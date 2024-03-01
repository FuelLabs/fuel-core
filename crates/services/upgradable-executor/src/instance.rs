use fuel_core_storage::{
    column::Column,
    kv_store::{
        KeyValueInspect,
        Value,
    },
};
use fuel_core_types::{
    blockchain::primitives::DaBlockHeight,
    fuel_tx::Transaction,
    fuel_vm::checked_transaction::Checked,
    services::executor::{
        Error as ExecutorError,
        Result as ExecutorResult,
    },
};
use fuel_core_wasm_executor::{
    fuel_core_executor::{
        executor::{
            ExecutionBlockWithSource,
            ExecutionOptions,
        },
        ports::{
            MaybeCheckedTransaction,
            RelayerPort,
            TransactionsSource,
        },
    },
    utils::{
        pack_exists_size_result,
        unpack_ptr_and_len,
        ReturnType,
    },
};
use std::{
    collections::HashMap,
    sync::Arc,
};
use wasmtime::{
    AsContextMut,
    Caller,
    Engine,
    Func,
    Linker,
    Memory,
    Module,
    Store,
};

trait CallerHelper {
    fn write(&mut self, ptr: u32, encoded: &[u8]);
}

impl<'a> CallerHelper for Caller<'a, ExecutionState> {
    fn write(&mut self, ptr: u32, encoded: &[u8]) {
        let memory = self.data_mut().memory.expect("Memory is initialized; qed");
        let mut store = self.as_context_mut();
        memory
            .write(&mut store, ptr as usize, encoded)
            .expect("Should write into the memory unless we've changed the implementation on the WASM side");
    }
}

struct ExecutionState {
    memory: Option<Memory>,
    next_transaction: HashMap<u32, Vec<Vec<u8>>>,
    relayer_events: HashMap<DaBlockHeight, Value>,
}

pub struct Instance<Stage = ()> {
    store: Store<ExecutionState>,
    stage: Stage,
}

pub struct Created {
    linker: Linker<ExecutionState>,
}

impl Instance {
    pub fn new(engine: &Engine) -> Instance<Created> {
        Instance {
            store: Store::new(
                engine,
                ExecutionState {
                    memory: None,
                    next_transaction: Default::default(),
                    relayer_events: Default::default(),
                },
            ),
            stage: Created {
                linker: Linker::new(engine),
            },
        }
    }
}

pub struct InputData {
    linker: Linker<ExecutionState>,
    input_component_size: u32,
    input_options_size: u32,
}

impl Instance<Created> {
    pub fn add_input_data(
        mut self,
        block: ExecutionBlockWithSource<()>,
        options: ExecutionOptions,
    ) -> ExecutorResult<Instance<InputData>> {
        let encoded_block = postcard::to_allocvec(&block).map_err(|e| {
            ExecutorError::Other(format!(
                "Failed encoding of the block for `input` function: {}",
                e
            ))
        })?;
        let encoded_block_size = encoded_block.len() as u32;

        let input = Func::wrap(
            &mut self.store,
            move |mut caller: Caller<'_, ExecutionState>, out_ptr: u32, out_len: u32| {
                assert_eq!(out_len, encoded_block_size);
                caller.write(out_ptr, &encoded_block);
            },
        );
        self.stage
            .linker
            .define(&self.store, "host_v0", "input_component", input)
            .map_err(|e| {
                ExecutorError::Other(format!(
                    "Failed definition of the `input_component` function: {}",
                    e
                ))
            })?;

        let encoded_options = postcard::to_allocvec(&options).map_err(|e| {
            ExecutorError::Other(format!(
                "Failed encoding of the execution options for `input_options` function: {}",
                e
            ))
        })?;
        let encoded_encoded_options_size = encoded_options.len() as u32;

        let input = Func::wrap(
            &mut self.store,
            move |mut caller: Caller<'_, ExecutionState>, out_ptr: u32, out_len: u32| {
                assert_eq!(out_len, encoded_encoded_options_size);
                caller.write(out_ptr, &encoded_options);
            },
        );
        self.stage
            .linker
            .define(&self.store, "host_v0", "input_options", input)
            .map_err(|e| {
                ExecutorError::Other(format!(
                    "Failed definition of the `input_options` function: {}",
                    e
                ))
            })?;
        Ok(Instance {
            store: self.store,
            stage: InputData {
                linker: self.stage.linker,
                input_component_size: encoded_block_size,
                input_options_size: encoded_encoded_options_size,
            },
        })
    }
}

pub struct Source {
    linker: Linker<ExecutionState>,
    input_component_size: u32,
    input_options_size: u32,
}

impl Instance<InputData> {
    pub fn add_source<TxSource>(
        mut self,
        source: Option<TxSource>,
    ) -> ExecutorResult<Instance<Source>>
    where
        TxSource: TransactionsSource + Send + Sync + 'static,
    {
        let source = source.map(|source| Arc::new(source));
        let peek_next_txs_size = Func::wrap(
            &mut self.store,
            move |mut caller: Caller<'_, ExecutionState>, gas_limit: u64| -> u32 {
                let Some(source) = source.clone() else {
                    return 0;
                };

                let txs: Vec<_> = source
                    .next(gas_limit)
                    .into_iter()
                    .map(|tx| match tx {
                        MaybeCheckedTransaction::CheckedTransaction(checked) => {
                            let checked: Checked<Transaction> = checked.into();
                            let (tx, _) = checked.into();
                            tx
                        }
                        MaybeCheckedTransaction::Transaction(tx) => tx,
                    })
                    .collect();

                let encoded_txs = postcard::to_allocvec(&txs).expect("Should encode txs");
                let encoded_size = encoded_txs.len() as u32;

                caller
                    .data_mut()
                    .next_transaction
                    .entry(encoded_size)
                    .or_default()
                    .push(encoded_txs);
                encoded_size
            },
        );
        self.stage
            .linker
            .define(
                &self.store,
                "host_v0",
                "peek_next_txs_size",
                peek_next_txs_size,
            )
            .map_err(|e| {
                ExecutorError::Other(format!(
                    "Failed definition of the `peek_next_txs_size` function: {}",
                    e
                ))
            })?;

        let consume_next_txs = Func::wrap(
            &mut self.store,
            move |mut caller: Caller<'_, ExecutionState>,
                  output_ptr: u32,
                  output_size: u32| {
                let encoded = caller
                    .data_mut()
                    .next_transaction
                    .get_mut(&output_size)
                    .and_then(|vector| vector.pop())
                    .unwrap_or_default();

                caller.write(output_ptr, &encoded);
            },
        );
        self.stage
            .linker
            .define(&self.store, "host_v0", "consume_next_txs", consume_next_txs)
            .map_err(|e| {
                ExecutorError::Other(format!(
                    "Failed definition of the `consume_next_txs` function: {}",
                    e
                ))
            })?;

        Ok(Instance {
            store: self.store,
            stage: Source {
                linker: self.stage.linker,
                input_component_size: self.stage.input_component_size,
                input_options_size: self.stage.input_options_size,
            },
        })
    }
}

pub struct Storage {
    linker: Linker<ExecutionState>,
    input_component_size: u32,
    input_options_size: u32,
}

impl Instance<Source> {
    pub fn add_storage<S>(mut self, storage: S) -> ExecutorResult<Instance<Storage>>
    where
        S: KeyValueInspect<Column = Column> + Send + Sync + 'static,
    {
        let arc_storage = Arc::new(storage);

        let storage = arc_storage.clone();
        let storage_size_of_value = Func::wrap(
            &mut self.store,
            move |caller: Caller<'_, ExecutionState>,
                  key_ptr: u32,
                  key_len: u32,
                  column: u32|
                  -> u64 {
                // TODO: Add support for new columns during upgrades.
                let column = fuel_core_storage::column::Column::try_from(column)
                    .expect("Unknown column");

                let (ptr, len) = (key_ptr as usize, key_len as usize);
                let memory = caller.data().memory.expect("Memory was initialized above");
                let key = &memory.data(&caller)[ptr..ptr + len];
                if let Ok(value) = storage.size_of_value(key, column) {
                    pack_exists_size_result(
                        value.is_some(),
                        value.unwrap_or_default() as u32,
                        0,
                    )
                } else {
                    pack_exists_size_result(false, 0, 0)
                }
            },
        );
        self.stage
            .linker
            .define(
                &self.store,
                "host_v0",
                "storage_size_of_value",
                storage_size_of_value,
            )
            .map_err(|e| {
                ExecutorError::Other(format!(
                    "Failed definition of the `storage_size_of_value` function: {}",
                    e
                ))
            })?;

        let storage = arc_storage.clone();
        let storage_get = Func::wrap(
            &mut self.store,
            move |mut caller: Caller<'_, ExecutionState>,
                  key_ptr: u32,
                  key_len: u32,
                  column: u32,
                  out_ptr: u32,
                  out_len: u32|
                  -> u32 {
                // TODO: Add support for new columns during upgrades.
                let column = fuel_core_storage::column::Column::try_from(column)
                    .expect("Unknown column");
                let (ptr, len) = (key_ptr as usize, key_len as usize);
                let memory = caller.data().memory.expect("Memory was initialized above");
                let key = &memory.data(&caller)[ptr..ptr + len];
                if let Ok(value) = storage.get(key, column) {
                    let value = value.expect(
                        "The WASM calls `get` only after `storage_size_of_value` return `Some`",
                    );
                    assert_eq!(value.len(), out_len as usize);
                    caller.write(out_ptr, value.as_slice());
                    0
                } else {
                    1
                }
            },
        );
        self.stage
            .linker
            .define(&self.store, "host_v0", "storage_get", storage_get)
            .map_err(|e| {
                ExecutorError::Other(format!(
                    "Failed definition of the `storage_get` function: {}",
                    e
                ))
            })?;

        Ok(Instance {
            store: self.store,
            stage: Storage {
                linker: self.stage.linker,
                input_component_size: self.stage.input_component_size,
                input_options_size: self.stage.input_options_size,
            },
        })
    }
}

pub struct Relayer {
    linker: Linker<ExecutionState>,
    input_component_size: u32,
    input_options_size: u32,
}

impl Instance<Storage> {
    pub fn add_relayer<R>(mut self, relayer: R) -> ExecutorResult<Instance<Relayer>>
    where
        R: RelayerPort + Send + Sync + 'static,
    {
        let arc_relayer = Arc::new(relayer);

        let relayer = arc_relayer.clone();
        let relayer_enabled = Func::wrap(
            &mut self.store,
            move |_: Caller<'_, ExecutionState>| -> u32 { relayer.enabled() as u32 },
        );
        self.stage
            .linker
            .define(&self.store, "host_v0", "relayer_enabled", relayer_enabled)
            .map_err(|e| {
                ExecutorError::Other(format!(
                    "Failed definition of the `relayer_enabled` function: {}",
                    e
                ))
            })?;

        let relayer = arc_relayer.clone();
        let relayer_size_of_events = Func::wrap(
            &mut self.store,
            move |mut caller: Caller<'_, ExecutionState>, da_block_height: u64| -> u64 {
                let da_block_height: DaBlockHeight = da_block_height.into();

                if let Some(encoded_event) =
                    caller.data().relayer_events.get(&da_block_height)
                {
                    pack_exists_size_result(true, encoded_event.len() as u32, 0)
                } else {
                    let events = relayer.get_events(&da_block_height);

                    if let Ok(events) = events {
                        let encoded_events =
                            postcard::to_allocvec(&events).expect("Should encode events");
                        let encoded_size = encoded_events.len() as u32;
                        caller
                            .data_mut()
                            .relayer_events
                            .insert(da_block_height, encoded_events.into());
                        pack_exists_size_result(true, encoded_size, 0)
                    } else {
                        pack_exists_size_result(false, 0, 1)
                    }
                }
            },
        );
        self.stage
            .linker
            .define(
                &self.store,
                "host_v0",
                "relayer_size_of_events",
                relayer_size_of_events,
            )
            .map_err(|e| {
                ExecutorError::Other(format!(
                    "Failed definition of the `relayer_size_of_events` function: {}",
                    e
                ))
            })?;

        let relayer_get_events = Func::wrap(
            &mut self.store,
            move |mut caller: Caller<'_, ExecutionState>,
                  da_block_height: u64,
                  out_ptr: u32| {
                let da_block_height: DaBlockHeight = da_block_height.into();
                let encoded_events = caller
                    .data()
                    .relayer_events
                    .get(&da_block_height)
                    .expect("The `relayer_size_of_events` should be called before `relayer_get_events`").clone();
                caller.write(out_ptr, encoded_events.as_ref());
            },
        );
        self.stage
            .linker
            .define(
                &self.store,
                "host_v0",
                "relayer_get_events",
                relayer_get_events,
            )
            .map_err(|e| {
                ExecutorError::Other(format!(
                    "Failed definition of the `relayer_get_events` function: {}",
                    e
                ))
            })?;

        Ok(Instance {
            store: self.store,
            stage: Relayer {
                linker: self.stage.linker,
                input_component_size: self.stage.input_component_size,
                input_options_size: self.stage.input_options_size,
            },
        })
    }
}

impl Instance<Relayer> {
    pub fn run(self, module: &Module) -> ReturnType {
        self.internal_run(module).map_err(|e| {
            ExecutorError::Other(format!("Error with WASM initialization: {}", e))
        })?
    }

    fn internal_run(mut self, module: &Module) -> anyhow::Result<ReturnType> {
        let instance = self
            .stage
            .linker
            .instantiate(&mut self.store, module)
            .map_err(|e| {
                anyhow::anyhow!("Failed to instantiate the module: {}", e.to_string())
            })?;

        let memory_export =
            instance
                .get_export(&mut self.store, "memory")
                .ok_or_else(|| {
                    anyhow::anyhow!("memory is not exported under `memory` name")
                })?;

        let memory = memory_export.into_memory().ok_or_else(|| {
            anyhow::anyhow!("the `memory` export should have memory type")
        })?;
        self.store.data_mut().memory = Some(memory);

        let run = instance
            .get_typed_func::<(u32, u32), u64>(&mut self.store, "execute")
            .map_err(|e| {
                anyhow::anyhow!("Failed to get the `execute` function: {}", e.to_string())
            })?;
        let result = run.call(
            &mut self.store,
            (
                self.stage.input_component_size,
                self.stage.input_options_size,
            ),
        )?;

        let (ptr, len) = unpack_ptr_and_len(result);
        let (ptr, len) = (ptr as usize, len as usize);
        let memory = self
            .store
            .data()
            .memory
            .expect("Memory was initialized above");
        let slice = &memory.data(&self.store)[ptr..ptr + len];

        postcard::from_bytes(slice).map_err(|e| anyhow::anyhow!(e))
    }
}
