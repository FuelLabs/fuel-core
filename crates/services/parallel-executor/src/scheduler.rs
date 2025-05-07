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
//! This can be done because we assume that the transaction pool is sending us transactions that are alTransactionsReadyForPickup correctly verified.
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
    column::Column,
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
    fuel_vm::checked_transaction::CheckedTransaction,
    services::executor::Error as ExecutorError,
};
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
    number_of_workers: usize,
}

#[derive(Debug, Clone, Default)]
pub struct ContractsChanges {
    contracts_changes: HashMap<ContractId, u64>,
    latest_id: u64,
    changes_storage: HashMap<u64, Changes>,
}

impl ContractsChanges {
    pub fn new() -> Self {
        Self {
            contracts_changes: HashMap::new(),
            changes_storage: HashMap::new(),
            latest_id: 0,
        }
    }

    pub fn add_changes(&mut self, contract_ids: Vec<ContractId>, changes: Changes) {
        let id = self.latest_id;
        self.latest_id += 1;
        for contract_id in contract_ids {
            self.contracts_changes.insert(contract_id, id);
        }
        self.changes_storage.insert(id, changes);
    }

    pub fn get_changes(&self, contract_id: &ContractId) -> Option<&Changes> {
        self.contracts_changes
            .get(contract_id)
            .and_then(|id| self.changes_storage.get(id))
    }

    pub fn extract_all_contracts_changes(&mut self) -> Vec<Changes> {
        let mut changes = vec![];
        for id in 0..self.latest_id {
            if let Some(change) = self.changes_storage.remove(&id) {
                changes.push(change);
            }
        }
        self.contracts_changes.clear();
        changes
    }

    pub fn clear(&mut self) {
        self.contracts_changes.clear();
        self.changes_storage.clear();
        self.latest_id = 0;
    }
}

pub struct Scheduler<TxSource, S> {
    /// Config
    config: Config,
    /// Transaction source to ask for new transactions
    transaction_source: TxSource,
    /// Database transaction for the execution
    storage: S,
    /// Runtime to run the workers
    runtime: Option<Runtime>,
    /// List of available workers
    current_available_workers: VecDeque<usize>,
    /// All contracts changes
    contracts_changes: ContractsChanges,
    /// Current contracts being executed
    current_executing_contracts: HashSet<ContractId>,
    /// Current execution tasks
    current_execution_tasks:
        FuturesUnordered<tokio::task::JoinHandle<WorkSessionExecutionResult>>,
    // All executed transactions batch associated with their id
    execution_results: HashMap<usize, WorkSessionSavedData>,
    /// Current scheduler state
    state: SchedulerState,
    /// Total maximum of transactions left
    tx_left: u16,
    /// Total maximum of byte size left
    tx_size_left: u32,
}

type SkippedTransactions = Vec<CheckedTransaction>;

struct WorkSessionExecutionResult {
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
    /// Batch of transactions (included skipped ones) useful to re-execute them in case of fallback skipped
    txs: Vec<CheckedTransaction>,
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
    /// The transactions of the batch
    txs: Vec<CheckedTransaction>,
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
enum SchedulerState {
    /// Ready for a new worker to get some transactions
    TransactionsReadyForPickup,
    /// Waiting for a new transaction to be added to the transaction source
    WaitingForNewTransaction,
    /// Waiting for a worker to finish because we have filtered transactions
    WaitingForWorker,
}

// Shutdown the tokio runtime to avoid panic if executor is already
// used from another tokio runtime
impl<TxSource, D> Drop for Scheduler<TxSource, D> {
    fn drop(&mut self) {
        if let Some(runtime) = self.runtime.take() {
            runtime.shutdown_background();
        }
    }
}

impl<TxSource, S> Scheduler<TxSource, S>
where
    TxSource: TransactionsSource,
    S: Storage,
{
    pub fn new(config: Config, transaction_source: TxSource, storage: S) -> Self {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(config.number_of_workers)
            .enable_all()
            .build()
            .unwrap();

        Self {
            transaction_source,
            runtime: Some(runtime),
            tx_left: config.block_transaction_count_limit,
            tx_size_left: config.block_transaction_size_limit,
            current_available_workers: (0..config.number_of_workers).collect(),
            config,
            storage,
            current_execution_tasks: FuturesUnordered::new(),
            execution_results: HashMap::new(),
            state: SchedulerState::TransactionsReadyForPickup,
            contracts_changes: ContractsChanges::new(),
            current_executing_contracts: HashSet::new(),
        }
    }

    fn reset(&mut self) {
        self.tx_left = self.config.block_transaction_count_limit;
        self.tx_size_left = self.config.block_transaction_size_limit;
        self.current_available_workers = (0..self.config.number_of_workers).collect();
        self.current_executing_contracts.clear();
        self.execution_results.clear();
        self.contracts_changes.clear();
        self.current_execution_tasks = FuturesUnordered::new();
        self.state = SchedulerState::TransactionsReadyForPickup;
    }

    pub async fn run(&mut self) -> Result<StorageChanges, SchedulerError> {
        let new_tx_notifier = self.transaction_source.get_new_transactions_notifier();
        let now = tokio::time::Instant::now();
        let deadline = now + self.config.total_execution_time;
        let mut nb_batch_created = 0;
        let mut nb_transactions = 0;
        let initial_gas_per_worker = self
            .config
            .block_gas_limit
            .checked_div(self.config.number_of_workers as u64)
            .ok_or(SchedulerError::InternalError(
                "Invalid block gas limit".to_string(),
            ))?;

        'outer: loop {
            if self.is_worker_idling() {
                let batch =
                    self.ask_new_transactions_batch(now, initial_gas_per_worker)?;
                let batch_len = batch.len() as u16;

                if batch.is_empty() {
                    continue 'outer;
                }

                self.execute_batch(batch, nb_batch_created, nb_transactions)?;

                nb_batch_created += 1;
                nb_transactions += batch_len;
            } else {
                tokio::select! {
                    _ = new_tx_notifier.notified() => {
                        self.new_executable_transactions();
                    }
                    result = self.current_execution_tasks.next() => {
                        match result {
                            Some(Ok(res)) => {
                                if !res.skipped_tx.is_empty() {
                                    self.sequential_fallback(res.batch_id, res.txs).await;
                                    continue;
                                }
                                self.register_execution_result(res);
                            }
                            _ => {
                                return Err(SchedulerError::InternalError(
                                    "Worker execution failed".to_string(),
                                ));
                            }
                        }
                    }
                    _ = tokio::time::sleep_until(deadline) => {
                        break 'outer;
                    }
                }
            }
        }

        self.wait_all_execution_tasks().await?;

        let res = self.verify_coherency_and_merge_results(nb_batch_created)?;

        self.reset();
        Ok(res)
    }

    fn is_worker_idling(&self) -> bool {
        !self.current_available_workers.is_empty()
            && self.state == SchedulerState::TransactionsReadyForPickup
    }

    fn new_executable_transactions(&mut self) {
        self.state = SchedulerState::TransactionsReadyForPickup;
    }

    fn ask_new_transactions_batch(
        &mut self,
        start_execution_time: tokio::time::Instant,
        initial_gas: u64,
    ) -> Result<Vec<CheckedTransaction>, SchedulerError> {
        let worker_id = self.current_available_workers.pop_front().ok_or(
            SchedulerError::InternalError("No available workers".to_string()),
        )?;
        let spent_time = start_execution_time.elapsed();
        // Time left in percentage to have the gas percentage left
        // TODO: Maybe avoid as u32
        let current_gas = initial_gas
            * (self.config.total_execution_time.as_millis() as u64
                - spent_time.as_millis() as u64)
            / self.config.total_execution_time.as_millis() as u64;

        let (batch, filtered) = self.transaction_source.get_executable_transactions(
            current_gas,
            self.tx_left,
            self.tx_size_left,
            // TODO: Move and return the filter instead of cloning
            Filter {
                excluded_contract_ids: self.current_executing_contracts.clone(),
            },
        );

        if batch.is_empty() {
            if filtered == TransactionFiltered::Filtered {
                self.state = SchedulerState::WaitingForWorker;
            } else {
                self.state = SchedulerState::WaitingForNewTransaction;
            }
            self.current_available_workers.push_back(worker_id);
        }

        self.tx_size_left -= batch.iter().map(|tx| tx.size()).sum::<usize>() as u32;
        self.tx_left -= batch.len() as u16;
        Ok(batch)
    }

    fn execute_batch(
        &mut self,
        batch: Vec<CheckedTransaction>,
        batch_id: usize,
        _start_idx_txs: u16,
    ) -> Result<(), SchedulerError> {
        let (contracts_used, coins_used) = get_contracts_and_coins_used(&batch);
        let runtime = self.runtime.as_ref().unwrap();
        self.current_execution_tasks.push(runtime.spawn({
            let mut used_contracts_changes = vec![];
            for contract in contracts_used.iter() {
                self.current_executing_contracts.insert(*contract);
                if let Some(changes) = self.contracts_changes.get_changes(contract) {
                    used_contracts_changes.push(changes);
                }
            }
            let contracts_used = contracts_used.clone();
            async move {
                // TODO: Execute the batch of transactions
                WorkSessionExecutionResult {
                    batch_id,
                    changes: Changes::default(),
                    coins_created: vec![],
                    coins_used,
                    contracts_used,
                    skipped_tx: vec![],
                    txs: batch,
                }
            }
        }));
        Ok(())
    }

    fn register_execution_result(&mut self, mut res: WorkSessionExecutionResult) {
        for contract in res.contracts_used.iter() {
            self.current_executing_contracts.remove(contract);
        }
        if self.state == SchedulerState::WaitingForWorker {
            self.state = SchedulerState::TransactionsReadyForPickup;
        }

        // Is it useful ?
        // Did I listed all column ?
        // Need future proof
        let mut tmp_contracts_changes = HashMap::new();
        for column in [
            Column::ContractsRawCode,
            Column::ContractsState,
            Column::ContractsLatestUtxo,
            Column::ContractsAssets,
            Column::ContractsAssetsMerkleData,
            Column::ContractsAssetsMerkleMetadata,
            Column::ContractsStateMerkleData,
            Column::ContractsStateMerkleMetadata,
        ] {
            let column = column.as_u32();
            if let Some(changes) = res.changes.remove(&column) {
                tmp_contracts_changes.insert(column, changes);
            }
        }
        self.contracts_changes
            .add_changes(res.contracts_used, tmp_contracts_changes);
        self.execution_results.insert(
            res.batch_id,
            WorkSessionSavedData {
                changes: res.changes,
                coins_created: res.coins_created,
                coins_used: res.coins_used,
                txs: res.txs,
            },
        );
    }

    async fn wait_all_execution_tasks(&mut self) -> Result<(), SchedulerError> {
        let tolerance_execution_time_overflow = self.config.total_execution_time / 10;
        let now = tokio::time::Instant::now();

        // We have reached the deadline
        // We need to merge the states of all the workers
        while !self.current_execution_tasks.is_empty() {
            match self.current_execution_tasks.next().await {
                Some(Ok(res)) => {
                    if !res.skipped_tx.is_empty() {
                        self.sequential_fallback(res.batch_id, res.txs).await;
                        break;
                    } else {
                        self.execution_results.insert(
                            res.batch_id,
                            WorkSessionSavedData {
                                changes: res.changes,
                                coins_created: res.coins_created,
                                coins_used: res.coins_used,
                                txs: res.txs,
                            },
                        );
                    }
                }
                Some(Err(_)) => {
                    return Err(SchedulerError::InternalError(
                        "Worker execution failed".to_string(),
                    ));
                }
                None => {}
            }
        }

        if now.elapsed() > tolerance_execution_time_overflow {
            tracing::warn!(
                "Execution time exceeded the limit by: {}ms",
                now.elapsed().as_millis()
            );
        }
        Ok(())
    }

    fn verify_coherency_and_merge_results(
        &mut self,
        nb_batch: usize,
    ) -> Result<StorageChanges, SchedulerError> {
        let mut storage_changes = vec![];

        let mut compiled_created_coins = CoinDependencyChainVerifier::new();
        for batch_id in 0..nb_batch {
            if let Some(changes) = self.execution_results.remove(&batch_id) {
                compiled_created_coins
                    .register_coins_created(batch_id, changes.coins_created);
                compiled_created_coins.verify_coins_used(
                    batch_id,
                    changes.coins_used,
                    &self.storage,
                )?;
                storage_changes.push(changes.changes);
            } else {
                return Err(SchedulerError::InternalError(format!(
                    "Batch {batch_id} not found in the execution results"
                )));
            }
        }
        storage_changes.extend(self.contracts_changes.extract_all_contracts_changes());
        Ok(StorageChanges::ChangesList(storage_changes))
    }

    // Wait for all the workers to finish gather all theirs transactions
    // re-execute them in one worker without skipped one. We also need to
    // fetch all the possible executed and stored batch after the lowest batch_id we gonna
    // re-execute.
    // Tell the TransactionSource that this transaction is skipped
    // to avoid sending new transactions that depend on it (using preconfirmation squeeze out)
    async fn sequential_fallback(
        &mut self,
        batch_id: usize,
        mut txs: Vec<CheckedTransaction>,
    ) {
        let current_execution_tasks = std::mem::take(&mut self.current_execution_tasks);
        let mut lower_batch_id = batch_id;
        let mut higher_batch_id = batch_id;
        let mut all_txs_by_batch_id = HashMap::new();
        for future in current_execution_tasks {
            match future.await {
                Ok(res) => {
                    all_txs_by_batch_id.insert(res.batch_id, res.txs);
                    if res.batch_id < lower_batch_id {
                        lower_batch_id = res.batch_id;
                    }
                    if res.batch_id > higher_batch_id {
                        higher_batch_id = res.batch_id;
                    }
                }
                Err(_) => {
                    tracing::error!("Worker execution failed");
                }
            }
        }

        let mut all_txs = vec![];
        for id in lower_batch_id..higher_batch_id {
            if let Some(txs) = all_txs_by_batch_id.remove(&id) {
                all_txs.extend(txs);
            } else if let Some(res) = self.execution_results.remove(&id) {
                all_txs.extend(res.txs);
            } else if id == batch_id {
                // Ordering of transactions is important so we need to place this code here
                // which avoid to just move, but it's fine because we should only trigger this once
                all_txs.extend(std::mem::take(&mut txs));
            } else {
                tracing::error!("Batch {id} not found in the execution results");
            }
        }
        // TODO: Execute the transactions sequentially

        // Save execution results for all batch id with empty data
        // to not break the batch chain
        for id in lower_batch_id..higher_batch_id {
            self.execution_results.insert(
                id,
                WorkSessionSavedData {
                    changes: Changes::default(),
                    coins_created: vec![],
                    coins_used: vec![],
                    txs: vec![],
                },
            );
        }
        // Save the execution results for the current batch
        self.execution_results.insert(
            batch_id,
            WorkSessionSavedData {
                changes: Changes::default(),
                coins_created: vec![],
                coins_used: vec![],
                txs: all_txs,
            },
        );
    }
}

struct CoinDependencyChainVerifier {
    coins_registered: HashMap<UtxoId, (usize, usize)>,
}

impl CoinDependencyChainVerifier {
    fn new() -> Self {
        Self {
            coins_registered: HashMap::new(),
        }
    }

    fn register_coins_created(
        &mut self,
        batch_id: usize,
        coins_created: Vec<(UtxoId, usize)>,
    ) {
        for (coin, idx) in coins_created {
            self.coins_registered.insert(coin, (batch_id, idx));
        }
    }

    fn verify_coins_used<S>(
        &self,
        batch_id: usize,
        coins_used: Vec<(UtxoId, usize)>,
        storage: &S,
    ) -> Result<(), SchedulerError>
    where
        S: Storage,
    {
        // TODO: Maybe we also want to verify the amount
        for (coin, idx) in coins_used {
            match storage.get_coin(&coin) {
                Ok(Some(_)) => {
                    // Coin is in the database
                }
                Ok(None) => {
                    // Coin is not in the database
                    match self.coins_registered.get(&coin) {
                        Some((coin_creation_batch_id, coin_creation_tx_idx)) => {
                            // Coin is in the block
                            if coin_creation_batch_id <= &batch_id
                                && coin_creation_tx_idx <= &idx
                            {
                                // Coin is created in a batch that is before the current one
                            } else {
                                // Coin is created in a batch that is after the current one
                                return Err(SchedulerError::InternalError(format!(
                                    "Coin {coin} is created in a batch that is after the current one"
                                )));
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
        Ok(())
    }
}

fn get_contracts_and_coins_used(
    batch: &[CheckedTransaction],
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
