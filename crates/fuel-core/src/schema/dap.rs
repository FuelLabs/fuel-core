use crate::{
    database::{
        transaction::DatabaseTransaction,
        vm_database::VmDatabase,
        Database,
    },
    schema::scalars::U64,
};
use async_graphql::{
    Context,
    Object,
    SchemaBuilder,
    ID,
};
use fuel_core_storage::{
    not_found,
    InterpreterStorage,
};
use fuel_core_types::{
    fuel_asm::{
        Instruction,
        RegisterId,
        Word,
    },
    fuel_tx::{
        Buildable,
        ConsensusParameters,
        Executable,
        Script,
        Transaction,
    },
    fuel_vm::{
        checked_transaction::{
            CheckedTransaction,
            IntoChecked,
        },
        consts,
        Interpreter,
        InterpreterError,
    },
};
use futures::lock::Mutex;
use std::{
    collections::HashMap,
    io,
    sync,
};
use tracing::{
    debug,
    trace,
};
use uuid::Uuid;

use fuel_core_types::fuel_vm::state::DebugEval;

pub struct Config {
    /// `true` means that debugger functionality is enabled.
    debug_enabled: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ConcreteStorage {
    vm: HashMap<ID, Interpreter<VmDatabase, Script>>,
    tx: HashMap<ID, Vec<Script>>,
    db: HashMap<ID, DatabaseTransaction>,
    params: ConsensusParameters,
}

impl ConcreteStorage {
    pub fn new(params: ConsensusParameters) -> Self {
        Self {
            params,
            ..Default::default()
        }
    }

    pub fn register(&self, id: &ID, register: RegisterId) -> Option<Word> {
        self.vm
            .get(id)
            .and_then(|vm| vm.registers().get(register).copied())
    }

    pub fn memory(&self, id: &ID, start: usize, size: usize) -> Option<&[u8]> {
        let (end, overflow) = start.overflowing_add(size);
        if overflow || end > consts::VM_MAX_RAM as usize {
            return None
        }

        self.vm.get(id).map(|vm| &vm.memory()[start..end])
    }

    pub fn init(
        &mut self,
        txs: &[Script],
        storage: DatabaseTransaction,
    ) -> anyhow::Result<ID> {
        let id = Uuid::new_v4();
        let id = ID::from(id);

        let vm_database = Self::vm_database(&storage)?;
        let tx = Self::dummy_tx();
        let checked_tx = tx
            .into_checked_basic(vm_database.block_height()?, &self.params)
            .map_err(|e| anyhow::anyhow!(e))?;
        self.tx
            .get_mut(&id)
            .map(|tx| tx.extend_from_slice(txs))
            .unwrap_or_else(|| {
                self.tx.insert(id.clone(), txs.to_owned());
            });

        let mut vm = Interpreter::with_storage(vm_database, (&self.params).into());
        vm.transact(checked_tx).map_err(|e| anyhow::anyhow!(e))?;
        self.vm.insert(id.clone(), vm);
        self.db.insert(id.clone(), storage);

        Ok(id)
    }

    pub fn kill(&mut self, id: &ID) -> bool {
        self.tx.remove(id);
        self.vm.remove(id);
        self.db.remove(id).is_some()
    }

    pub fn reset(&mut self, id: &ID, storage: DatabaseTransaction) -> anyhow::Result<()> {
        let vm_database = Self::vm_database(&storage)?;
        let tx = self
            .tx
            .get(id)
            .and_then(|tx| tx.first())
            .cloned()
            .unwrap_or(Self::dummy_tx());

        let checked_tx = tx
            .into_checked_basic(vm_database.block_height()?, &self.params)
            .map_err(|e| anyhow::anyhow!(e))?;

        let mut vm = Interpreter::with_storage(vm_database, (&self.params).into());
        vm.transact(checked_tx).map_err(|e| anyhow::anyhow!(e))?;
        self.vm.insert(id.clone(), vm).ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, "The VM instance was not found")
        })?;
        self.db.insert(id.clone(), storage);
        Ok(())
    }

    pub fn exec(&mut self, id: &ID, op: Instruction) -> anyhow::Result<()> {
        self.vm
            .get_mut(id)
            .map(|vm| vm.instruction(op))
            .transpose()
            .map_err(|e| anyhow::anyhow!(e))?
            .map(|_| ())
            .ok_or_else(|| anyhow::anyhow!("The VM instance was not found"))
    }

    fn vm_database(storage: &DatabaseTransaction) -> anyhow::Result<VmDatabase> {
        let block = storage
            .get_current_block()?
            .ok_or(not_found!("Block for VMDatabase"))?
            .into_owned();

        let vm_database = VmDatabase::new(
            storage.as_ref().clone(),
            &block.header().consensus,
            // TODO: Use a real coinbase address
            Default::default(),
        );

        Ok(vm_database)
    }

    fn dummy_tx() -> Script {
        // Create `Script` transaction with dummy coin
        let mut tx = Script::default();
        tx.add_unsigned_coin_input(
            Default::default(),
            &Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
        );
        tx.add_witness(vec![].into());
        tx
    }
}

pub type GraphStorage = sync::Arc<Mutex<ConcreteStorage>>;

#[derive(Default)]
pub struct DapQuery;
#[derive(Default)]
pub struct DapMutation;

pub fn init<Q, M, S>(
    schema: SchemaBuilder<Q, M, S>,
    params: ConsensusParameters,
    debug_enabled: bool,
) -> SchemaBuilder<Q, M, S> {
    schema
        .data(GraphStorage::new(Mutex::new(ConcreteStorage::new(params))))
        .data(Config { debug_enabled })
}

fn require_debug(ctx: &Context<'_>) -> async_graphql::Result<()> {
    let config = ctx.data_unchecked::<Config>();

    if config.debug_enabled {
        Ok(())
    } else {
        Err(async_graphql::Error::new("The 'debug' feature is disabled"))
    }
}

#[Object]
impl DapQuery {
    async fn register(
        &self,
        ctx: &Context<'_>,
        id: ID,
        register: U64,
    ) -> async_graphql::Result<U64> {
        require_debug(ctx)?;
        ctx.data_unchecked::<GraphStorage>()
            .lock()
            .await
            .register(&id, register.0 as RegisterId)
            .ok_or_else(|| async_graphql::Error::new("Invalid register identifier"))
            .map(|val| val.into())
    }

    async fn memory(
        &self,
        ctx: &Context<'_>,
        id: ID,
        start: U64,
        size: U64,
    ) -> async_graphql::Result<String> {
        require_debug(ctx)?;
        ctx.data_unchecked::<GraphStorage>()
            .lock()
            .await
            .memory(&id, start.0 as usize, size.0 as usize)
            .ok_or_else(|| async_graphql::Error::new("Invalid memory range"))
            .and_then(|mem| Ok(serde_json::to_string(mem)?))
    }
}

#[Object]
impl DapMutation {
    async fn start_session(&self, ctx: &Context<'_>) -> async_graphql::Result<ID> {
        require_debug(ctx)?;
        trace!("Initializing new interpreter");

        let db = ctx.data_unchecked::<Database>();

        let id = ctx
            .data_unchecked::<GraphStorage>()
            .lock()
            .await
            .init(&[], db.transaction())?;

        debug!("Session {:?} initialized", id);

        Ok(id)
    }

    async fn end_session(
        &self,
        ctx: &Context<'_>,
        id: ID,
    ) -> async_graphql::Result<bool> {
        require_debug(ctx)?;
        let existed = ctx.data_unchecked::<GraphStorage>().lock().await.kill(&id);

        debug!("Session {:?} dropped with result {}", id, existed);

        Ok(existed)
    }

    async fn reset(&self, ctx: &Context<'_>, id: ID) -> async_graphql::Result<bool> {
        require_debug(ctx)?;
        let db = ctx.data_unchecked::<Database>();

        ctx.data_unchecked::<GraphStorage>()
            .lock()
            .await
            .reset(&id, db.transaction())?;

        debug!("Session {:?} was reset", id);

        Ok(true)
    }

    async fn execute(
        &self,
        ctx: &Context<'_>,
        id: ID,
        op: String,
    ) -> async_graphql::Result<bool> {
        require_debug(ctx)?;
        trace!("Execute encoded op {}", op);

        let op: Instruction = serde_json::from_str(op.as_str())?;

        trace!("Op decoded to {:?}", op);

        let storage = ctx.data_unchecked::<GraphStorage>().clone();
        let result = storage.lock().await.exec(&id, op).is_ok();

        debug!("Op {:?} executed with result {}", op, result);

        Ok(result)
    }

    async fn set_single_stepping(
        &self,
        ctx: &Context<'_>,
        id: ID,
        enable: bool,
    ) -> async_graphql::Result<bool> {
        require_debug(ctx)?;
        trace!("Set single stepping to {} for VM {:?}", enable, id);

        let mut locked = ctx.data_unchecked::<GraphStorage>().lock().await;
        let vm = locked
            .vm
            .get_mut(&id)
            .ok_or_else(|| async_graphql::Error::new("VM not found"))?;

        vm.set_single_stepping(enable);
        Ok(enable)
    }

    async fn set_breakpoint(
        &self,
        ctx: &Context<'_>,
        id: ID,
        breakpoint: gql_types::Breakpoint,
    ) -> async_graphql::Result<bool> {
        require_debug(ctx)?;
        trace!("Continue execution of VM {:?}", id);

        let mut locked = ctx.data_unchecked::<GraphStorage>().lock().await;
        let vm = locked
            .vm
            .get_mut(&id)
            .ok_or_else(|| async_graphql::Error::new("VM not found"))?;

        vm.set_breakpoint(breakpoint.into());
        Ok(true)
    }

    async fn start_tx(
        &self,
        ctx: &Context<'_>,
        id: ID,
        tx_json: String,
    ) -> async_graphql::Result<gql_types::RunResult> {
        require_debug(ctx)?;
        trace!("Spawning a new VM instance");

        let tx: Transaction = serde_json::from_str(&tx_json)
            .map_err(|_| async_graphql::Error::new("Invalid transaction JSON"))?;

        let mut locked = ctx.data_unchecked::<GraphStorage>().lock().await;

        let db = locked.db.get(&id).ok_or("Invalid debugging session ID")?;

        let checked_tx = tx
            .into_checked_basic(db.latest_height()?, &locked.params)?
            .into();

        let vm = locked
            .vm
            .get_mut(&id)
            .ok_or_else(|| async_graphql::Error::new("VM not found"))?;

        match checked_tx {
            CheckedTransaction::Script(script) => {
                let state_ref = vm.transact(script).map_err(|err| {
                    async_graphql::Error::new(format!("Transaction failed: {err:?}"))
                })?;

                let json_receipts = state_ref
                    .receipts()
                    .iter()
                    .map(|r| {
                        serde_json::to_string(&r).expect("JSON serialization failed")
                    })
                    .collect();

                let dbgref = state_ref.state().debug_ref();
                Ok(gql_types::RunResult {
                    state: match dbgref {
                        Some(_) => gql_types::RunState::Breakpoint,
                        None => gql_types::RunState::Completed,
                    },
                    breakpoint: dbgref.and_then(|d| match d {
                        DebugEval::Continue => None,
                        DebugEval::Breakpoint(bp) => Some(bp.into()),
                    }),
                    json_receipts,
                })
            }
            CheckedTransaction::Create(create) => {
                vm.deploy(create).map_err(|err| {
                    async_graphql::Error::new(format!(
                        "Transaction deploy failed: {err:?}"
                    ))
                })?;

                Ok(gql_types::RunResult {
                    state: gql_types::RunState::Completed,
                    breakpoint: None,
                    json_receipts: vec![],
                })
            }
            CheckedTransaction::Mint(_) => {
                Err(async_graphql::Error::new("`Mint` is not supported"))
            }
        }
    }

    async fn continue_tx(
        &self,
        ctx: &Context<'_>,
        id: ID,
    ) -> async_graphql::Result<gql_types::RunResult> {
        require_debug(ctx)?;
        trace!("Continue execution of VM {:?}", id);

        let mut locked = ctx.data_unchecked::<GraphStorage>().lock().await;
        let vm = locked
            .vm
            .get_mut(&id)
            .ok_or_else(|| async_graphql::Error::new("VM not found"))?;

        let receipt_count_before = vm.receipts().len();

        let state = match vm.resume() {
            Ok(state) => state,
            // The transaction was already completed earlier, so it cannot be resumed
            Err(InterpreterError::DebugStateNotInitialized) => {
                return Ok(gql_types::RunResult {
                    state: gql_types::RunState::Completed,
                    breakpoint: None,
                    json_receipts: Vec::new(),
                })
            }
            // The transaction was already completed earlier, so it cannot be resumed
            Err(err) => {
                return Err(async_graphql::Error::new(format!("VM error: {err:?}")))
            }
        };

        let json_receipts = vm
            .receipts()
            .iter()
            .skip(receipt_count_before)
            .map(|r| serde_json::to_string(&r).expect("JSON serialization failed"))
            .collect();

        let dbgref = state.debug_ref();

        Ok(gql_types::RunResult {
            state: match dbgref {
                Some(_) => gql_types::RunState::Breakpoint,
                None => gql_types::RunState::Completed,
            },
            breakpoint: dbgref.and_then(|d| match d {
                DebugEval::Continue => None,
                DebugEval::Breakpoint(bp) => Some(bp.into()),
            }),
            json_receipts,
        })
    }
}

mod gql_types {
    //! GraphQL type wrappers
    use async_graphql::*;

    use crate::schema::scalars::{
        ContractId,
        U64,
    };

    use fuel_core_types::fuel_vm::Breakpoint as FuelBreakpoint;

    #[derive(Debug, Clone, Copy, InputObject)]
    pub struct Breakpoint {
        contract: ContractId,
        pc: U64,
    }

    impl From<&FuelBreakpoint> for Breakpoint {
        fn from(bp: &FuelBreakpoint) -> Self {
            Self {
                contract: (*bp.contract()).into(),
                pc: U64(bp.pc()),
            }
        }
    }

    impl From<Breakpoint> for FuelBreakpoint {
        fn from(bp: Breakpoint) -> Self {
            Self::new(bp.contract.into(), bp.pc.0)
        }
    }

    /// A separate `Breakpoint` type to be used as an output, as a single
    /// type cannot act as both input and output type in async-graphql
    #[derive(Debug, Clone, Copy, SimpleObject)]
    pub struct OutputBreakpoint {
        contract: ContractId,
        pc: U64,
    }

    impl From<&FuelBreakpoint> for OutputBreakpoint {
        fn from(bp: &FuelBreakpoint) -> Self {
            Self {
                contract: (*bp.contract()).into(),
                pc: U64(bp.pc()),
            }
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Enum)]
    pub enum RunState {
        /// All breakpoints have been processed, and the program has terminated
        Completed,
        /// Stopped on a breakpoint
        Breakpoint,
    }

    #[derive(Debug, Clone, SimpleObject)]
    pub struct RunResult {
        pub state: RunState,
        pub breakpoint: Option<OutputBreakpoint>,
        pub json_receipts: Vec<String>,
    }
}
