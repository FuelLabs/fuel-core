use fuel_core_executor::{
    executor::ExecutionOptions,
    ports::{
        MaybeCheckedTransaction,
        RelayerPort,
        TransactionsSource,
    },
};
use fuel_core_storage::{
    column::Column,
    kv_store::{
        KeyValueInspect,
        Value,
    },
};
use fuel_core_types::{
    blockchain::{
        block::Block,
        primitives::DaBlockHeight,
    },
    fuel_tx::Transaction,
    fuel_vm::checked_transaction::Checked,
    services::{
        block_producer::Components,
        executor::{
            Error as ExecutorError,
            Result as ExecutorResult,
        },
    },
};
use fuel_core_wasm_executor::utils::{
    pack_exists_size_result,
    unpack_ptr_and_len,
    InputSerializationType,
    ReturnType,
    WasmSerializationBlockTypes,
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
    /// Writes the encoded data to the memory at the provided pointer.
    fn write(&mut self, ptr: u32, encoded: &[u8]) -> anyhow::Result<()>;
}

impl<'a> CallerHelper for Caller<'a, ExecutionState> {
    fn write(&mut self, ptr: u32, encoded: &[u8]) -> anyhow::Result<()> {
        let memory = self.data_mut().memory.expect("Memory is initialized; qed");
        let mut store = self.as_context_mut();
        memory
            .write(&mut store, ptr as usize, encoded)
            .map_err(|e| anyhow::anyhow!("Failed to write to the memory: {}", e))
    }
}

/// The state used by the host functions provided for the WASM executor.
struct ExecutionState {
    /// The memory used by the WASM module.
    memory: Option<Memory>,
    next_transactions: HashMap<u32, Vec<Vec<u8>>>,
    relayer_events: HashMap<DaBlockHeight, Value>,
}

/// The WASM instance has several stages of initialization.
/// When the instance is fully initialized, it is possible to run it.
///
/// Each stage has a state. Advancing the stage may add a new state that
/// will later be used by the next stages or a `run` function.
///
/// Currently, the order of the definition of the host functions doesn't matter,
/// but later, the data from previous stages may be used to initialize new stages.
pub struct Instance<Stage = ()> {
    store: Store<ExecutionState>,
    linker: Linker<ExecutionState>,
    stage: Stage,
}

/// The stage indicates that the instance is fresh and newly created.
pub struct Created;

impl Instance {
    pub fn new(engine: &Engine) -> Instance<Created> {
        Instance {
            store: Store::new(
                engine,
                ExecutionState {
                    memory: None,
                    next_transactions: Default::default(),
                    relayer_events: Default::default(),
                },
            ),
            linker: Linker::new(engine),
            stage: Created {},
        }
    }
}

impl<Stage> Instance<Stage> {
    const LATEST_HOST_MODULE: &'static str = "host_v0";

    /// Adds a new host function to the instance.
    pub fn add_method(&mut self, method: &str, func: Func) -> ExecutorResult<()> {
        self.linker
            .define(&self.store, Self::LATEST_HOST_MODULE, method, func)
            .map_err(|e| {
                ExecutorError::Other(format!(
                    "Failed definition of the `{}` function: {}",
                    method, e
                ))
            })?;
        Ok(())
    }
}

/// This stage adds host functions to get transactions from the transaction source.
pub struct Source;

impl Instance<Created> {
    /// Adds host functions for the `source`.
    pub fn add_source<TxSource>(
        mut self,
        source: Option<TxSource>,
    ) -> ExecutorResult<Instance<Source>>
    where
        TxSource: TransactionsSource + Send + Sync + 'static,
    {
        let source = source.map(|source| Arc::new(source));
        let peek_next_txs_size = self.peek_next_txs_size(source.clone());
        self.add_method("peek_next_txs_size", peek_next_txs_size)?;

        let consume_next_txs = self.consume_next_txs();
        self.add_method("consume_next_txs", consume_next_txs)?;

        Ok(Instance {
            store: self.store,
            linker: self.linker,
            stage: Source {},
        })
    }

    pub fn no_source(mut self) -> ExecutorResult<Instance<Source>> {
        let peek_next_txs_size = self.no_source_peek_next_txs_size();
        self.add_method("peek_next_txs_size", peek_next_txs_size)?;

        let consume_next_txs = self.consume_next_txs();
        self.add_method("consume_next_txs", consume_next_txs)?;

        Ok(Instance {
            store: self.store,
            linker: self.linker,
            stage: Source {},
        })
    }

    fn peek_next_txs_size<TxSource>(&mut self, source: Option<Arc<TxSource>>) -> Func
    where
        TxSource: TransactionsSource + Send + Sync + 'static,
    {
        let closure = move |mut caller: Caller<'_, ExecutionState>,
                            gas_limit: u64|
              -> anyhow::Result<u32> {
            let Some(source) = source.clone() else {
                return Ok(0);
            };

            let txs: Vec<_> = source
                .next(gas_limit)
                .into_iter()
                .map(|tx| match tx {
                    MaybeCheckedTransaction::CheckedTransaction(checked, _) => {
                        let checked: Checked<Transaction> = checked.into();
                        let (tx, _) = checked.into();
                        tx
                    }
                    MaybeCheckedTransaction::Transaction(tx) => tx,
                })
                .collect();

            let encoded_txs = postcard::to_allocvec(&txs).map_err(|e| {
                ExecutorError::Other(format!(
                    "Failed encoding of the transactions for `peek_next_txs_size` function: {}",
                    e
                ))
            })?;
            let encoded_size = u32::try_from(encoded_txs.len()).map_err(|e| {
                ExecutorError::Other(format!(
                    "The encoded transactions are more than `u32::MAX`. We support only wasm32: {}",
                    e
                ))
            })?;

            caller
                .data_mut()
                .next_transactions
                .entry(encoded_size)
                .or_default()
                .push(encoded_txs);
            Ok(encoded_size)
        };

        Func::wrap(&mut self.store, closure)
    }

    fn no_source_peek_next_txs_size(&mut self) -> Func {
        let closure =
            move |_: Caller<'_, ExecutionState>, _: u64| -> anyhow::Result<u32> { Ok(0) };

        Func::wrap(&mut self.store, closure)
    }

    fn consume_next_txs(&mut self) -> Func {
        let closure = move |mut caller: Caller<'_, ExecutionState>,
                            output_ptr: u32,
                            output_size: u32|
              -> anyhow::Result<()> {
            let encoded = caller
                .data_mut()
                .next_transactions
                .get_mut(&output_size)
                .and_then(|vector| vector.pop())
                .unwrap_or_default();

            caller.write(output_ptr, &encoded)
        };

        Func::wrap(&mut self.store, closure)
    }
}

/// This stage adds host functions for the storage.
pub struct Storage;

impl Instance<Source> {
    /// Adds getters to the `storage`.
    pub fn add_storage<S>(mut self, storage: S) -> ExecutorResult<Instance<Storage>>
    where
        S: KeyValueInspect<Column = Column> + Send + Sync + 'static,
    {
        let storage = Arc::new(storage);

        let storage_size_of_value = self.storage_size_of_value(storage.clone());
        self.add_method("storage_size_of_value", storage_size_of_value)?;

        let storage_get = self.storage_get(storage);
        self.add_method("storage_get", storage_get)?;

        Ok(Instance {
            store: self.store,
            linker: self.linker,
            stage: Storage {},
        })
    }

    fn storage_size_of_value<S>(&mut self, storage: Arc<S>) -> Func
    where
        S: KeyValueInspect<Column = Column> + Send + Sync + 'static,
    {
        let closure = move |caller: Caller<'_, ExecutionState>,
                            key_ptr: u32,
                            key_len: u32,
                            column: u32|
              -> anyhow::Result<u64> {
            let column = fuel_core_storage::column::Column::try_from(column)
                .map_err(|e| anyhow::anyhow!("Unknown column: {}", e))?;

            let (ptr, len) = (key_ptr as usize, key_len as usize);
            let memory = caller
                .data()
                .memory
                .expect("Memory was initialized above; qed");

            let key = &memory.data(&caller)[ptr..ptr.saturating_add(len)];
            if let Ok(value) = storage.size_of_value(key, column) {
                let size = u32::try_from(value.unwrap_or_default()).map_err(|e| {
                    anyhow::anyhow!(
                        "The size of the value is more than `u32::MAX`. We support only wasm32: {}",
                        e
                    )
                })?;

                Ok(pack_exists_size_result(value.is_some(), size, 0))
            } else {
                Ok(pack_exists_size_result(false, 0, 0))
            }
        };

        Func::wrap(&mut self.store, closure)
    }

    fn storage_get<S>(&mut self, storage: Arc<S>) -> Func
    where
        S: KeyValueInspect<Column = Column> + Send + Sync + 'static,
    {
        let closure = move |mut caller: Caller<'_, ExecutionState>,
                            key_ptr: u32,
                            key_len: u32,
                            column: u32,
                            out_ptr: u32,
                            out_len: u32|
              -> anyhow::Result<u32> {
            let column = fuel_core_storage::column::Column::try_from(column)
                .map_err(|e| anyhow::anyhow!("Unknown column: {}", e))?;
            let (ptr, len) = (key_ptr as usize, key_len as usize);
            let memory = caller
                .data()
                .memory
                .expect("Memory was initialized above; qed");

            let key = &memory.data(&caller)[ptr..ptr.saturating_add(len)];
            if let Ok(value) = storage.get(key, column) {
                let value = value.ok_or(anyhow::anyhow!("\
                        The WASM executor should call `get` only after `storage_size_of_value`."))?;

                if value.len() != out_len as usize {
                    return Err(anyhow::anyhow!(
                        "The provided buffer size is not equal to the value size."
                    ));
                }

                caller.write(out_ptr, value.as_slice())?;
                Ok(0)
            } else {
                Ok(1)
            }
        };

        Func::wrap(&mut self.store, closure)
    }
}

/// This stage adds host functions for the relayer.
pub struct Relayer;

impl Instance<Storage> {
    /// Adds host functions for the `relayer`.
    pub fn add_relayer<R>(mut self, relayer: R) -> ExecutorResult<Instance<Relayer>>
    where
        R: RelayerPort + Send + Sync + 'static,
    {
        let relayer = Arc::new(relayer);

        let relayer_enabled = self.relayer_enabled(relayer.clone());
        self.add_method("relayer_enabled", relayer_enabled)?;

        let relayer_size_of_events = self.relayer_size_of_events(relayer);
        self.add_method("relayer_size_of_events", relayer_size_of_events)?;

        let relayer_get_events = self.relayer_get_events();
        self.add_method("relayer_get_events", relayer_get_events)?;

        Ok(Instance {
            store: self.store,
            linker: self.linker,
            stage: Relayer {},
        })
    }

    fn relayer_enabled<R>(&mut self, relayer: Arc<R>) -> Func
    where
        R: RelayerPort + Send + Sync + 'static,
    {
        let closure =
            move |_: Caller<'_, ExecutionState>| -> u32 { relayer.enabled() as u32 };

        Func::wrap(&mut self.store, closure)
    }

    fn relayer_size_of_events<R>(&mut self, relayer: Arc<R>) -> Func
    where
        R: RelayerPort + Send + Sync + 'static,
    {
        let closure = move |mut caller: Caller<'_, ExecutionState>,
                            da_block_height: u64|
              -> anyhow::Result<u64> {
            let da_block_height: DaBlockHeight = da_block_height.into();

            if let Some(encoded_events) =
                caller.data().relayer_events.get(&da_block_height)
            {
                let encoded_size = u32::try_from(encoded_events.len()).map_err(|e| {
                    anyhow::anyhow!(
                        "The size of encoded events is more than `u32::MAX`. We support only wasm32: {}",
                        e
                    )
                })?;
                Ok(pack_exists_size_result(true, encoded_size, 0))
            } else {
                let events = relayer.get_events(&da_block_height);

                if let Ok(events) = events {
                    let encoded_events =
                        postcard::to_allocvec(&events).map_err(|e| anyhow::anyhow!(e))?;
                    let encoded_size =
                        u32::try_from(encoded_events.len()).map_err(|e| {
                            anyhow::anyhow!(
                                "The size of encoded events is more than `u32::MAX`. We support only wasm32: {}",
                                e
                            )
                        })?;

                    caller
                        .data_mut()
                        .relayer_events
                        .insert(da_block_height, encoded_events.into());
                    Ok(pack_exists_size_result(true, encoded_size, 0))
                } else {
                    Ok(pack_exists_size_result(false, 0, 1))
                }
            }
        };

        Func::wrap(&mut self.store, closure)
    }

    fn relayer_get_events(&mut self) -> Func {
        let closure = move |mut caller: Caller<'_, ExecutionState>,
                            da_block_height: u64,
                            out_ptr: u32|
              -> anyhow::Result<()> {
            let da_block_height: DaBlockHeight = da_block_height.into();
            let encoded_events = caller
                .data()
                .relayer_events
                .get(&da_block_height)
                .ok_or(anyhow::anyhow!(
                    "The `relayer_size_of_events` should be called before `relayer_get_events`"
                ))?
                .clone();

            caller.write(out_ptr, encoded_events.as_ref())?;
            Ok(())
        };

        Func::wrap(&mut self.store, closure)
    }
}

/// This stage adds host functions - getter for the input data like `Block` and `ExecutionOptions`.
pub struct InputData {
    input_component_size: u32,
}

impl Instance<Relayer> {
    /// Adds getters for the `block` and `options`.
    pub fn add_production_input_data(
        self,
        components: Components<()>,
        options: ExecutionOptions,
    ) -> ExecutorResult<Instance<InputData>> {
        let input = InputSerializationType::V1 {
            block: WasmSerializationBlockTypes::Production(components),
            options,
        };
        self.add_input_data(input)
    }

    pub fn add_dry_run_input_data(
        self,
        components: Components<()>,
        options: ExecutionOptions,
    ) -> ExecutorResult<Instance<InputData>> {
        let input = InputSerializationType::V1 {
            block: WasmSerializationBlockTypes::DryRun(components),
            options,
        };
        self.add_input_data(input)
    }

    //
    pub fn add_validation_input_data(
        self,
        block: &Block,
        options: ExecutionOptions,
    ) -> ExecutorResult<Instance<InputData>> {
        let input = InputSerializationType::V1 {
            block: WasmSerializationBlockTypes::Validation(block),
            options,
        };
        self.add_input_data(input)
    }

    fn add_input_data(
        mut self,
        input: InputSerializationType,
    ) -> ExecutorResult<Instance<InputData>> {
        let encoded_input = postcard::to_allocvec(&input).map_err(|e| {
            ExecutorError::Other(format!(
                "Failed encoding of the input for `input` function: {}",
                e
            ))
        })?;
        let encoded_input_size = u32::try_from(encoded_input.len()).map_err(|e| {
            ExecutorError::Other(format!(
                "The encoded input is more than `u32::MAX`. We support only wasm32: {}",
                e
            ))
        })?;

        let input = self.input(encoded_input, encoded_input_size);
        self.add_method("input", input)?;

        Ok(Instance {
            store: self.store,
            linker: self.linker,
            stage: InputData {
                input_component_size: encoded_input_size,
            },
        })
    }

    fn input(&mut self, encoded_input: Vec<u8>, encoded_input_size: u32) -> Func {
        let closure = move |mut caller: Caller<'_, ExecutionState>,
                            out_ptr: u32,
                            out_len: u32|
              -> anyhow::Result<()> {
            if out_len != encoded_input_size {
                return Err(anyhow::anyhow!(
                    "The provided buffer size is not equal to the encoded input size."
                ));
            }

            caller.write(out_ptr, &encoded_input)
        };

        Func::wrap(&mut self.store, closure)
    }
}

impl Instance<InputData> {
    /// Runs the WASM instance from the compiled `Module`.
    pub fn run(self, module: &Module) -> ExecutorResult<ReturnType> {
        self.internal_run(module).map_err(|e| {
            ExecutorError::Other(format!("Error with WASM initialization: {}", e))
        })
    }

    fn internal_run(mut self, module: &Module) -> anyhow::Result<ReturnType> {
        let instance = self
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
            .get_typed_func::<u32, u64>(&mut self.store, "execute")
            .map_err(|e| {
                anyhow::anyhow!("Failed to get the `execute` function: {}", e.to_string())
            })?;
        let result = run.call(&mut self.store, self.stage.input_component_size)?;

        let (ptr, len) = unpack_ptr_and_len(result);
        let (ptr, len) = (ptr as usize, len as usize);
        let memory = self
            .store
            .data()
            .memory
            .expect("Memory was initialized above; qed");
        let slice = &memory.data(&self.store)[ptr..ptr.saturating_add(len)];

        postcard::from_bytes(slice).map_err(|e| anyhow::anyhow!(e))
    }
}
