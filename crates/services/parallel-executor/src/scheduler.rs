use std::collections::{
    HashMap,
    HashSet,
};

use fuel_core_storage::transactional::StorageChanges;
use fuel_core_types::fuel_tx::ContractId;
use fuel_core_upgradable_executor::executor::Executor as UpgradableExecutor;
use tokio::runtime::Runtime;

use crate::ports::{
    Filter,
    TransactionsSource,
};

/// The scheduler is responsible for managing the state of all the execution workers.
/// His goal is to gather transactions for the transaction source and organize their execution
/// through the different workers.
///
/// There is few rules that need to be followed in order to produce a valid execution result:
/// - The dependency chain of the input and output must be maintained across the block.
/// - The constraints of the block (maximum number of transactions, maximum size, maximum gas, etc.) must be respected.
///
/// Current design:
///
/// The scheduler creates multiple workers. For each of this workers, the scheduler will ask to the transaction source
/// to provide a batch of transactions that can be executed.
///
/// When a thread has finished his execution then it will notify the scheduler that will re-ask for a new batch to the transaction source.
/// This new batch mustn't contain any transaction that use a contract used in a batch of any other worker.
///
/// For transactions without contracts, they are treat them as independent transactions. A verification is done at the end for the coin dependency chain.
/// This can be done because we assume that the transaction pool is sending us transactions that are already correctly verified.
/// If we have a transaction that end up being skipped (only possible cause if consensus parameters changes) then we will have to
/// fallback a sequential execution of the transaction that used the skipped one as a dependency.

type WorkerID = usize;

pub struct Scheduler<TxSource> {
    /// The state of each worker
    workers_state: Vec<WorkerState>,
    /// Latest worker that executed a transaction with a specific contract
    contracts_info: HashMap<ContractId, WorkerID>,
    /// The list of contracts that are currently being executed for each worker (useful to filter them when asking to transaction source)
    executing_contracts: Vec<Vec<ContractId>>,
    /// Transaction source to ask for new transactions
    transaction_source: TxSource,
    /// Runtime to run the workers
    runtime: Option<Runtime>,
}

pub struct WorkerState {
    pub status: WorkerStatus,
    pub state: StorageChanges,
    pub gas_left: u64,
    pub tx_left: u16,
    pub tx_size_left: u32,
    // Maybe something that could wake the scheduler
}

pub enum WorkerStatus {
    /// The worker is waiting for a new batch of transactions
    Idle,
    /// The worker is currently executing a batch of transactions
    Executing,
    /// The worker is merging his state with another
    Merging,
}

// Shutdown the tokio runtime to avoid panic if executor is already
//   used from another tokio runtime
impl<TxSource> Drop for Scheduler<TxSource> {
    fn drop(&mut self) {
        if let Some(runtime) = self.runtime.take() {
            runtime.shutdown_background();
        }
    }
}

impl<TxSource> Scheduler<TxSource>
where
    TxSource: TransactionsSource,
{
    pub fn new(number_of_worker: usize, transaction_source: TxSource) -> Self {
        let mut workers_state = Vec::with_capacity(number_of_worker);
        for _ in 0..number_of_worker {
            workers_state.push(WorkerState {
                status: WorkerStatus::Idle,
                state: StorageChanges::default(),
                // TODO
                gas_left: 0,
                tx_left: 0,
                tx_size_left: 0,
            });
        }
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(number_of_worker)
            .enable_all()
            .build()
            .unwrap();

        Self {
            workers_state,
            contracts_info: HashMap::new(),
            executing_contracts: vec![Vec::new(); number_of_worker],
            transaction_source,
            runtime: Some(runtime),
        }
    }

    pub async fn run(&mut self) {
        let runtime = self.runtime.as_ref().unwrap();
        // All workers starts empty, so we fetch the first batch
        for state in self.workers_state.iter_mut() {
            let (batch, _) = self.transaction_source.get_executable_transactions(
                state.gas_left,
                state.tx_left,
                state.tx_size_left,
                Filter {
                    excluded_contract_ids: HashSet::new(),
                },
            );
            runtime.spawn(async move {})
        }
        // Waiting for the workers to notify us
    }
}
