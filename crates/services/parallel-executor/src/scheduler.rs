//! The scheduler is responsible for managing the state of all the execution workers.
//! His goal is to gather transactions for the transaction source and organize their execution
//! through the different workers.
//!
//! There is few rules that need to be followed in order to produce a valid execution result:
//! - The dependency chain of the input and output must be maintained across the block.
//! - The constraints of the block (maximum number of transactions, maximum size, maximum gas, etc.) must be respected.
//!
//! Current design:
//!
//! The scheduler creates multiple workers. For each of this workers, the scheduler will ask to the transaction source
//! to provide a batch of transactions that can be executed.
//!
//! When a thread has finished his execution then it will notify the scheduler that will re-ask for a new batch to the transaction source.
//! This new batch mustn't contain any transaction that use a contract used in a batch of any other worker.
//!
//! For transactions without contracts, they are treat them as independent transactions. A verification is done at the end for the coin dependency chain.
//! This can be done because we assume that the transaction pool is sending us transactions that are already correctly verified.
//! If we have a transaction that end up being skipped (only possible cause if consensus parameters changes) then we will have to
//! fallback a sequential execution of the transaction that used the skipped one as a dependency.

use std::{
    collections::{
        HashMap,
        HashSet,
        VecDeque,
    },
    time::Duration,
};

use ::futures::{
    StreamExt,
    stream::FuturesUnordered,
};
use fuel_core_storage::{
    Error as StorageError,
    transactional::{
        Changes,
        StorageChanges,
    },
};
use fuel_core_types::{
    blockchain::transaction::TransactionExt,
    fuel_tx::{
        ContractId,
        UtxoId,
    },
    services::executor::Error as ExecutorError,
};
use fuel_core_upgradable_executor::native_executor::ports::MaybeCheckedTransaction;
use tokio::runtime::Runtime;

use crate::ports::{
    Filter,
    Storage,
    TransactionFiltered,
    TransactionsSource,
};

/// Config for the scheduler
pub struct Config {
    block_gas_limit: u64,
    total_execution_time: Duration,
    block_transaction_size_limit: u32,
    block_transaction_count_limit: u16,
}

pub struct Scheduler<TxSource, D> {
    /// Config
    config: Config,
    /// The state of each worker
    workers_state: Vec<WorkerState>,
    /// Transaction source to ask for new transactions
    transaction_source: TxSource,
    /// Database transaction for the execution
    storage: D,
    /// Runtime to run the workers
    runtime: Option<Runtime>,
    /// Total maximum of transactions left
    tx_left: u16,
    /// Total maximum of byte size left
    tx_size_left: u32,
}

pub struct WorkerState {
    pub status: WorkerStatus,
    pub executing_contracts: Vec<ContractId>,
}

pub enum WorkerStatus {
    /// The worker is waiting for a new batch of transactions
    Idle,
    /// The worker is currently executing a batch of transactions
    Executing,
    /// The worker is merging his state with another
    Merging,
}

type SkippedTransactions = Vec<MaybeCheckedTransaction>;

struct WorkSessionExecutionResult {
    /// The id of the worker
    worker_id: usize,
    /// The id of the batch of transactions
    batch_id: usize,
    /// The changes made by the worker used to commit them to the database at the end of execution
    changes: Changes,
    /// The coins created by the worker used to verify the coin dependency chain at the end of execution
    /// We also store the index of the transaction in the batch in case the usage is in the same batch
    coins_created: Vec<(UtxoId, usize)>,
    /// The coins used by the worker used to verify the coin dependency chain at the end of execution
    /// We also store the index of the transaction in the batch in case the creation is in the same batch
    coins_used: Vec<(UtxoId, usize)>,
    /// Contracts used during the execution of the transactions to save the changes for future usage of
    /// the contracts
    contracts_used: Vec<ContractId>,
    /// The transactions that were skipped by the worker
    skipped_tx: SkippedTransactions,
}

struct WorkSessionSavedData {
    /// The changes made by the worker used to commit them to the database at the end of execution
    changes: Changes,
    /// The coins created by the worker used to verify the coin dependency chain at the end of execution
    /// We also store the index of the transaction in the batch in case the usage is in the same batch
    coins_created: Vec<(UtxoId, usize)>,
    /// The coins used by the worker used to verify the coin dependency chain at the end of execution
    /// We also store the index of the transaction in the batch in case the creation is in the same batch
    coins_used: Vec<(UtxoId, usize)>,
}

/// Error type for the scheduler
#[derive(Debug)]
pub enum SchedulerError {
    /// Error while executing the transactions
    ExecutionError(ExecutorError),
    /// Error while getting the transactions from the transaction source
    TransactionSourceError(String),
    /// Error while getting the coins from the storage
    StorageError(StorageError),
    /// Internal error
    InternalError(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum WaitingReason {
    /// Waiting for a worker to finish because we have filtered transactions
    WaitingForWorker,
    /// Waiting for a new transaction to be added to the transaction source
    WaitingForNewTransaction,
    /// No waiting reason
    NoWaiting,
}

// Shutdown the tokio runtime to avoid panic if executor is already
//   used from another tokio runtime
impl<TxSource, D> Drop for Scheduler<TxSource, D> {
    fn drop(&mut self) {
        if let Some(runtime) = self.runtime.take() {
            runtime.shutdown_background();
        }
    }
}

impl<TxSource, D> Scheduler<TxSource, D>
where
    TxSource: TransactionsSource,
    D: Storage,
{
    pub fn new(
        config: Config,
        number_of_worker: usize,
        transaction_source: TxSource,
        storage: D,
    ) -> Self {
        let mut workers_state = Vec::with_capacity(number_of_worker);
        for _ in 0..number_of_worker {
            workers_state.push(WorkerState {
                status: WorkerStatus::Idle,
                executing_contracts: vec![],
            });
        }
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(number_of_worker)
            .enable_all()
            .build()
            .unwrap();

        Self {
            workers_state,
            transaction_source,
            runtime: Some(runtime),
            tx_left: config.block_transaction_count_limit,
            tx_size_left: config.block_transaction_size_limit,
            config,
            storage,
        }
    }

    pub async fn run(&mut self) -> Result<StorageChanges, SchedulerError> {
        let runtime = self.runtime.as_ref().unwrap();
        let now = tokio::time::Instant::now();
        let mut waiting_reason = WaitingReason::NoWaiting;
        let new_tx_notifier = self.transaction_source.get_new_transactions_notifier();
        let deadline = now + self.config.total_execution_time;
        // All executed transactions batch associated with their id
        let mut execution_results = HashMap::new();
        // All contracts ids that have already been executed with their changes
        let mut contracts_changes: HashMap<ContractId, Changes> = HashMap::new();
        let mut nb_batch_created = 0;
        let mut _current_nb_transactions = 0;
        // Store the futures of all the workers to be triggered when one of them finish
        let mut futures = FuturesUnordered::new();
        let mut current_available_workers: VecDeque<usize> =
            (0..self.workers_state.len()).collect();
        let number_of_workers = self.workers_state.len();
        let initial_gas = self
            .config
            .block_gas_limit
            .checked_div(number_of_workers as u64)
            .ok_or(SchedulerError::InternalError(
                "Invalid block gas limit".to_string(),
            ))?;

        'outer: loop {
            // Check if we have any free workers
            if !current_available_workers.is_empty()
                && waiting_reason == WaitingReason::NoWaiting
            {
                // SAFETY: We know that we have at least one worker available (checked line above)
                let worker_id = current_available_workers.pop_front().unwrap();
                let contracts_currently_used = self
                    .workers_state
                    .iter()
                    .flat_map(|state| state.executing_contracts.clone())
                    .collect::<HashSet<_>>();
                let spent_time = now.elapsed();
                // Time left in percentage to have the gas percentage left
                let current_gas = initial_gas
                    * ((1u128
                        - spent_time.as_millis()
                            / self.config.total_execution_time.as_millis())
                        as u64);

                let (batch, filtered) =
                    self.transaction_source.get_executable_transactions(
                        current_gas,
                        self.tx_left,
                        self.tx_size_left,
                        Filter {
                            excluded_contract_ids: contracts_currently_used,
                        },
                    );

                if batch.is_empty() {
                    if filtered == TransactionFiltered::Filtered {
                        waiting_reason = WaitingReason::WaitingForWorker;
                    } else {
                        waiting_reason = WaitingReason::WaitingForNewTransaction;
                    }
                    current_available_workers.push_back(worker_id);
                    continue 'outer;
                }

                self.tx_left -= batch.len() as u16;
                // Useful to have a range of slots in the block to set tx pointers
                _current_nb_transactions += batch.len() as u16;
                self.tx_size_left -=
                    batch.iter().map(|tx| tx.size()).sum::<usize>() as u32;

                let (contracts_used, coins_used) = get_contracts_and_coins_used(&batch);

                futures.push(runtime.spawn({
                    let batch_id = nb_batch_created;
                    let mut used_contracts_changes = vec![];
                    for contract in contracts_used.iter() {
                        if let Some(changes) = contracts_changes.remove(contract) {
                            used_contracts_changes.push(changes);
                        }
                    }
                    let contracts_used = contracts_used.clone();
                    async move {
                        // TODO: Execute the batch of transactions
                        WorkSessionExecutionResult {
                            worker_id,
                            batch_id,
                            changes: Changes::default(),
                            coins_created: vec![],
                            coins_used,
                            contracts_used,
                            skipped_tx: vec![],
                        }
                    }
                }));
                nb_batch_created += 1;
                self.workers_state[worker_id].executing_contracts = contracts_used;
                self.workers_state[worker_id].status = WorkerStatus::Executing;
            } else {
                tokio::select! {
                    // We have new transactions to execute
                    _ = new_tx_notifier.notified() => {
                        waiting_reason = WaitingReason::NoWaiting;
                    }
                    result = futures.next() => {
                        match result {
                            Some(Ok(WorkSessionExecutionResult { worker_id, batch_id, changes, coins_created, coins_used, contracts_used, skipped_tx })) => {
                                if !skipped_tx.is_empty() {
                                    // TODO: Handle the skipped transactions
                                    // Wait for all the workers to finish gather all theirs transactions
                                    // re-execute them in one worker without skipped one.
                                    // Tell the TransactionSource that this transaction is skipped
                                    // to avoid sending new transactions that depend on it (using preconfirmation squeeze out)
                                }
                                // Update the state of the worker
                                self.workers_state[worker_id].status = WorkerStatus::Idle;
                                self.workers_state[worker_id].executing_contracts.clear();
                                if waiting_reason == WaitingReason::WaitingForWorker {
                                    waiting_reason = WaitingReason::NoWaiting;
                                }

                                // TODO: Save only the part that touch the contract to not clone everything
                                contracts_changes.extend(
                                    contracts_used
                                        .iter()
                                        .map(|contract| (*contract, changes.clone()))
                                );
                                execution_results.insert(
                                    batch_id,
                                    WorkSessionSavedData {
                                        changes,
                                        coins_created,
                                        coins_used,
                                    }
                                );
                            }
                            _ => {
                                return Err(SchedulerError::InternalError(
                                    "Worker execution failed".to_string(),
                                ));
                            }
                        }
                    }
                    // We have reached the deadline
                    _ = tokio::time::sleep_until(deadline) => {
                        // We have reached the deadline
                        break 'outer;
                    }
                }
            }
        }
        let tolerance_execution_time_overflow = self.config.total_execution_time / 10;
        let now = tokio::time::Instant::now();

        // We have reached the deadline
        // We need to merge the states of all the workers
        for future in futures {
            match future.await {
                Ok(res) => {
                    if !res.skipped_tx.is_empty() {
                        // TODO: Handle the skipped transactions
                        // Wait for all the workers to finish gather all theirs transactions
                        // re-execute them in one worker without skipped one.
                        // Tell the TransactionSource that this transaction is skipped
                        // to avoid sending new transactions that depend on it (using preconfirmation squeeze out)
                    }
                    execution_results.insert(
                        res.batch_id,
                        WorkSessionSavedData {
                            changes: res.changes,
                            coins_created: res.coins_created,
                            coins_used: res.coins_used,
                        },
                    );
                }
                Err(_) => {
                    return Err(SchedulerError::InternalError(
                        "Worker execution failed".to_string(),
                    ));
                }
            }
        }

        if now.elapsed() > tolerance_execution_time_overflow {
            tracing::warn!(
                "Execution time exceeded the limit by: {}ms",
                now.elapsed().as_millis()
            );
        }

        // Verify the coin dependency chain
        let mut storage_changes = vec![];
        let mut compiled_created_coins = HashMap::new();
        // TODO: Maybe we also want to verify the amount
        for batch_id in 0..nb_batch_created {
            if let Some(changes) = execution_results.remove(&batch_id) {
                for (coin, idx) in changes.coins_created {
                    compiled_created_coins.insert(coin, (batch_id, idx));
                }
                for (coin, idx) in changes.coins_used {
                    match self.storage.get_coin(&coin) {
                        Ok(Some(_)) => {
                            // Coin is in the database
                        }
                        Ok(None) => {
                            // Coin is not in the database
                            match compiled_created_coins.get(&coin) {
                                Some((coin_creation_batch_id, coin_creation_tx_idx)) => {
                                    // Coin is in the block
                                    if coin_creation_batch_id <= &batch_id
                                        && coin_creation_tx_idx <= &idx
                                    {
                                        // Coin is created in a batch that is before the current one
                                    } else {
                                        // Coin is created in a batch that is after the current one
                                        return Err(SchedulerError::InternalError(
                                            format!(
                                                "Coin {coin} is created in a batch that is after the current one"
                                            ),
                                        ));
                                    }
                                }
                                None => {
                                    return Err(SchedulerError::InternalError(format!(
                                        "Coin {coin} is not in the database and not created in the block"
                                    )));
                                }
                            }
                        }
                        Err(e) => {
                            return Err(SchedulerError::InternalError(format!(
                                "Error while getting coin {coin}: {e}"
                            )));
                        }
                    }
                }
                storage_changes.push(changes.changes);
            } else {
                return Err(SchedulerError::InternalError(format!(
                    "Batch {batch_id} not found in the execution results"
                )));
            }
        }

        Ok(StorageChanges::ChangesList(storage_changes))
    }
}

fn get_contracts_and_coins_used(
    batch: &[MaybeCheckedTransaction],
) -> (Vec<ContractId>, Vec<(UtxoId, usize)>) {
    let mut contracts_used = vec![];
    let mut coins_used = vec![];

    for (idx, tx) in batch.iter().enumerate() {
        let inputs = tx.inputs();
        for input in inputs.iter() {
            match input {
                fuel_core_types::fuel_tx::Input::Contract(contract) => {
                    contracts_used.push(contract.contract_id);
                }
                fuel_core_types::fuel_tx::Input::CoinSigned(coin) => {
                    coins_used.push((coin.utxo_id, idx));
                }
                fuel_core_types::fuel_tx::Input::CoinPredicate(coin) => {
                    coins_used.push((coin.utxo_id, idx));
                }
                _ => {}
            }
        }
    }

    (contracts_used, coins_used)
}
