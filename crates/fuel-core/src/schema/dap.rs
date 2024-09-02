use crate::{
    database::{
        database_description::on_chain::OnChain,
        Database,
        OnChainIterableKeyValueView,
    },
    fuel_core_graphql_api::api_service::ConsensusProvider,
    schema::scalars::{
        U32,
        U64,
    },
};
use anyhow::anyhow;
use async_graphql::{
    Context,
    Object,
    SchemaBuilder,
    ID,
};
use fuel_core_storage::{
    not_found,
    transactional::{
        AtomicView,
        IntoTransaction,
        StorageTransaction,
    },
    vm_storage::VmStorage,
    InterpreterStorage,
};
use fuel_core_types::{
    fuel_asm::{
        Instruction,
        RegisterId,
        Word,
    },
    fuel_tx::{
        field::{
            Policies,
            ScriptGasLimit,
            Witnesses,
        },
        policies::PolicyType,
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
        interpreter::{
            InterpreterParams,
            MemoryInstance,
        },
        state::DebugEval,
        Interpreter,
        InterpreterError,
    },
};
use futures::lock::Mutex;
use std::{
    collections::HashMap,
    io,
    sync,
    sync::Arc,
};
use tracing::{
    debug,
    trace,
};
use uuid::Uuid;

pub struct Config {
    /// `true` means that debugger functionality is enabled.
    debug_enabled: bool,
}

type FrozenDatabase = VmStorage<StorageTransaction<OnChainIterableKeyValueView>>;

#[derive(Default, Debug)]
pub struct ConcreteStorage {
    vm: HashMap<ID, Interpreter<MemoryInstance, FrozenDatabase, Script>>,
    tx: HashMap<ID, Vec<Script>>,
}

/// The gas price used for transactions in the debugger. It is set to 0 because
/// the debugger does not actually execute transactions, but only simulates
/// their execution.
const GAS_PRICE: u64 = 0;

impl ConcreteStorage {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&self, id: &ID, register: RegisterId) -> Option<Word> {
        self.vm
            .get(id)
            .and_then(|vm| vm.registers().get(register).copied())
    }

    pub fn memory(&self, id: &ID, start: usize, size: usize) -> Option<&[u8]> {
        self.vm
            .get(id)
            .and_then(|vm| vm.memory().read(start, size).ok())
    }

    pub fn init(
        &mut self,
        txs: &[Script],
        params: Arc<ConsensusParameters>,
        storage: &Database<OnChain>,
    ) -> anyhow::Result<ID> {
        let id = Uuid::new_v4();
        let id = ID::from(id);

        let vm_database = Self::vm_database(storage)?;
        let tx = Self::dummy_tx(params.tx_params().max_gas_per_tx() / 2);
        let checked_tx = tx
            .into_checked_basic(vm_database.block_height()?, &params)
            .map_err(|e| anyhow::anyhow!("{:?}", e))?;
        self.tx
            .get_mut(&id)
            .map(|tx| tx.extend_from_slice(txs))
            .unwrap_or_else(|| {
                self.tx.insert(id.clone(), txs.to_owned());
            });

        let gas_costs = params.gas_costs();
        let fee_params = params.fee_params();

        let ready_tx = checked_tx
            .into_ready(GAS_PRICE, gas_costs, fee_params)
            .map_err(|e| {
                anyhow!("Failed to apply dynamic values to checked tx: {:?}", e)
            })?;

        let interpreter_params = InterpreterParams::new(GAS_PRICE, params.as_ref());
        let mut vm = Interpreter::with_storage(
            MemoryInstance::new(),
            vm_database,
            interpreter_params,
        );
        vm.transact(ready_tx).map_err(|e| anyhow::anyhow!(e))?;
        self.vm.insert(id.clone(), vm);

        Ok(id)
    }

    pub fn kill(&mut self, id: &ID) -> bool {
        self.tx.remove(id);
        self.vm.remove(id).is_some()
    }

    pub fn reset(
        &mut self,
        id: &ID,
        params: Arc<ConsensusParameters>,
        storage: &Database<OnChain>,
    ) -> anyhow::Result<()> {
        let vm_database = Self::vm_database(storage)?;
        let tx = self
            .tx
            .get(id)
            .and_then(|tx| tx.first())
            .cloned()
            .unwrap_or(Self::dummy_tx(params.tx_params().max_gas_per_tx() / 2));

        let checked_tx = tx
            .into_checked_basic(vm_database.block_height()?, &params)
            .map_err(|e| anyhow::anyhow!("{:?}", e))?;

        let gas_costs = params.gas_costs();
        let fee_params = params.fee_params();

        let ready_tx = checked_tx
            .into_ready(GAS_PRICE, gas_costs, fee_params)
            .map_err(|e| {
                anyhow!("Failed to apply dynamic values to checked tx: {:?}", e)
            })?;

        let interpreter_params = InterpreterParams::new(GAS_PRICE, params.as_ref());
        let mut vm = Interpreter::with_storage(
            MemoryInstance::new(),
            vm_database,
            interpreter_params,
        );
        vm.transact(ready_tx).map_err(|e| anyhow::anyhow!(e))?;
        self.vm.insert(id.clone(), vm).ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, "The VM instance was not found")
        })?;
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

    fn vm_database(storage: &Database<OnChain>) -> anyhow::Result<FrozenDatabase> {
        let view = storage.latest_view()?;
        let block = view
            .get_current_block()?
            .ok_or(not_found!("Block for VMDatabase"))?;

        let vm_database = VmStorage::new(
            view.into_transaction(),
            block.header().consensus(),
            block.header().application(),
            // TODO: Use a real coinbase address
            Default::default(),
        );

        Ok(vm_database)
    }

    fn dummy_tx(gas_limit: u64) -> Script {
        // Create `Script` transaction with dummy coin
        let mut tx = Script::default();
        *tx.script_gas_limit_mut() = gas_limit;
        tx.add_unsigned_coin_input(
            Default::default(),
            &Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
        );
        tx.witnesses_mut().push(vec![].into());
        tx.policies_mut().set(PolicyType::MaxFee, Some(0));
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
    debug_enabled: bool,
) -> SchemaBuilder<Q, M, S> {
    schema
        .data(GraphStorage::new(Mutex::new(ConcreteStorage::new())))
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
    /// Read register value by index.
    async fn register(
        &self,
        ctx: &Context<'_>,
        id: ID,
        register: U32,
    ) -> async_graphql::Result<U64> {
        require_debug(ctx)?;
        ctx.data_unchecked::<GraphStorage>()
            .lock()
            .await
            .register(&id, register.0 as RegisterId)
            .ok_or_else(|| async_graphql::Error::new("Invalid register identifier"))
            .map(|val| val.into())
    }

    /// Read read a range of memory bytes.
    async fn memory(
        &self,
        ctx: &Context<'_>,
        id: ID,
        start: U32,
        size: U32,
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
    /// Initialize a new debugger session, returning its ID.
    /// A new VM instance is spawned for each session.
    /// The session is run in a separate database transaction,
    /// on top of the most recent node state.
    async fn start_session(&self, ctx: &Context<'_>) -> async_graphql::Result<ID> {
        require_debug(ctx)?;
        trace!("Initializing new interpreter");

        let db = ctx.data_unchecked::<Database>();
        let params = ctx
            .data_unchecked::<ConsensusProvider>()
            .latest_consensus_params();

        let id =
            ctx.data_unchecked::<GraphStorage>()
                .lock()
                .await
                .init(&[], params, db)?;

        debug!("Session {:?} initialized", id);

        Ok(id)
    }

    /// End debugger session.
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

    /// Reset the VM instance to the initial state.
    async fn reset(&self, ctx: &Context<'_>, id: ID) -> async_graphql::Result<bool> {
        require_debug(ctx)?;
        let db = ctx.data_unchecked::<Database>();
        let params = ctx
            .data_unchecked::<ConsensusProvider>()
            .latest_consensus_params();

        ctx.data_unchecked::<GraphStorage>()
            .lock()
            .await
            .reset(&id, params, db)?;

        debug!("Session {:?} was reset", id);

        Ok(true)
    }

    /// Execute a single fuel-asm instruction.
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

    /// Set single-stepping mode for the VM instance.
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

    /// Set a breakpoint for a VM instance.
    async fn set_breakpoint(
        &self,
        ctx: &Context<'_>,
        id: ID,
        breakpoint: gql_types::Breakpoint,
    ) -> async_graphql::Result<bool> {
        require_debug(ctx)?;
        trace!("Set breakpoint for VM {:?}", id);

        let mut locked = ctx.data_unchecked::<GraphStorage>().lock().await;
        let vm = locked
            .vm
            .get_mut(&id)
            .ok_or_else(|| async_graphql::Error::new("VM not found"))?;

        vm.set_breakpoint(breakpoint.into());
        Ok(true)
    }

    /// Run a single transaction in given session until it
    /// hits a breakpoint or completes.
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
        let params = ctx
            .data_unchecked::<ConsensusProvider>()
            .latest_consensus_params();

        let vm = locked
            .vm
            .get_mut(&id)
            .ok_or_else(|| async_graphql::Error::new("VM not found"))?;

        let checked_tx = tx
            .into_checked_basic(vm.as_ref().block_height()?, &params)
            .map_err(|err| anyhow::anyhow!("{:?}", err))?
            .into();

        let gas_costs = params.gas_costs();
        let fee_params = params.fee_params();

        match checked_tx {
            CheckedTransaction::Script(script) => {
                let ready_tx = script
                    .into_ready(GAS_PRICE, gas_costs, fee_params)
                    .map_err(|e| {
                        anyhow!("Failed to apply dynamic values to checked tx: {:?}", e)
                    })?;
                let state_ref = vm.transact(ready_tx).map_err(|err| {
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
            CheckedTransaction::Create(_) => {
                Err(async_graphql::Error::new("`Create` is not supported"))
            }
            CheckedTransaction::Mint(_) => {
                Err(async_graphql::Error::new("`Mint` is not supported"))
            }
            CheckedTransaction::Upgrade(_) => {
                Err(async_graphql::Error::new("`Upgrade` is not supported"))
            }
            CheckedTransaction::Upload(_) => {
                Err(async_graphql::Error::new("`Upload` is not supported"))
            }
            CheckedTransaction::Blob(_) => {
                Err(async_graphql::Error::new("`Blob` is not supported"))
            }
        }
    }

    /// Resume execution of the VM instance after a breakpoint.
    /// Runs until the next breakpoint or until the transaction completes.
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

    /// Breakpoint, defined as a tuple of contract ID and relative PC offset inside it
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
