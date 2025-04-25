use std::{
    collections::HashSet,
    time::Duration,
};

use ::futures::{
    StreamExt,
    stream::FuturesUnordered,
};
use fuel_core_storage::transactional::StorageChanges;
use fuel_core_types::fuel_tx::ContractId;
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

pub struct Config {
    block_gas_limit: u64,
    max_execution_time: Duration,
    block_transaction_size_limit: u32,
    block_transaction_count_limit: u16,
}

pub struct Scheduler<TxSource> {
    /// Config
    config: Config,
    /// The state of each worker
    workers_state: Vec<WorkerState>,
    /// The list of contracts that are currently being executed for each worker (useful to filter them when asking to transaction source)
    executing_contracts: Vec<Vec<ContractId>>,
    /// Transaction source to ask for new transactions
    transaction_source: TxSource,
    /// Runtime to run the workers
    runtime: Option<Runtime>,
    /// Total execution time left (used to determine gas_left)
    execution_time_left: u64,
    /// Total maximum of transactions left
    tx_left: u16,
    /// Total maximum of byte size left
    tx_size_left: u32,
}

pub struct WorkerState {
    pub status: WorkerStatus,
    pub state: StorageChanges,
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
    pub fn new(
        config: Config,
        number_of_worker: usize,
        transaction_source: TxSource,
    ) -> Self {
        let mut workers_state = Vec::with_capacity(number_of_worker);
        for _ in 0..number_of_worker {
            workers_state.push(WorkerState {
                status: WorkerStatus::Idle,
                state: StorageChanges::default(),
            });
        }
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(number_of_worker)
            .enable_all()
            .build()
            .unwrap();

        Self {
            workers_state,
            executing_contracts: vec![Vec::new(); number_of_worker],
            transaction_source,
            runtime: Some(runtime),
            // TODO: change AS
            execution_time_left: config.max_execution_time.as_millis() as u64,
            tx_left: config.block_transaction_count_limit,
            tx_size_left: config.block_transaction_size_limit,
            config,
        }
    }

    // TODO: Error type
    pub async fn run(&mut self) -> Result<(), String> {
        let runtime = self.runtime.as_ref().unwrap();
        let mut spent_time = Duration::ZERO;
        // Store the futures of all the workers to be triggered when one of them finish
        let mut futures = FuturesUnordered::new();

        // All workers starts empty, so we fetch the first batch
        let number_of_workers = self.workers_state.len();
        let initial_gas = self
            .config
            .block_gas_limit
            .checked_div(number_of_workers as u64)
            .ok_or("Invalid block gas limit")?;
        for (i, state) in self.workers_state.iter_mut().enumerate() {
            let (batch, _) = self.transaction_source.get_executable_transactions(
                initial_gas,
                self.tx_left,
                self.tx_size_left,
                Filter {
                    excluded_contract_ids: HashSet::new(),
                },
            );
            futures.push(runtime.spawn({
                let worker_id = i;
                async move {
                    // TODO: Execute the batch of transactions
                    (worker_id, StorageChanges::default())
                }
            }));
        }
        // Waiting for the workers to notify us
        while !futures.is_empty() {
            // Wait for the first worker to finish
            let result = futures.next().await;
            match result {
                Some(Ok((worker_id, changes))) => {
                    // Update the state of the worker
                    self.workers_state[worker_id].status = WorkerStatus::Idle;
                    self.workers_state[worker_id].state = changes;
                }
                _ => {
                    return Err("Worker failed".to_string());
                }
            }
            // Ask for new transactions to the transaction source
        }
        Ok(())
    }
}
