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
    sync::Arc,
    time::Duration,
};

use ::futures::{
    StreamExt,
    stream::FuturesUnordered,
};
use fuel_core_executor::{
    executor::{
        BlockExecutor,
        ExecutionData,
        ExecutionOptions,
    },
    ports::{
        PreconfirmationSenderPort,
        RelayerPort,
    },
};
use fuel_core_storage::{
    Error as StorageError,
    column::Column,
    kv_store::KeyValueInspect,
    transactional::{
        AtomicView,
        Changes,
        IntoTransaction,
        StorageChanges,
        StorageTransaction,
        WriteTransaction,
    },
};
use fuel_core_types::{
    blockchain::{
        header::PartialBlockHeader,
        transaction::TransactionExt,
    },
    fuel_tx::{
        ConsensusParameters,
        ContractId,
        MessageId,
        Output,
        Transaction,
        TxId,
        UtxoId,
    },
    fuel_types::BlockHeight,
    fuel_vm::checked_transaction::{
        CheckedTransaction,
        IntoChecked,
    },
    services::{
        block_producer::Components,
        executor::{
            Error as ExecutorError,
            Event,
            TransactionExecutionStatus,
        },
    },
};
use fxhash::FxHashMap;
use tokio::runtime::Runtime;

use crate::{
    checked_transaction_ext::CheckedTransactionExt,
    coin::CoinInBatch,
    column_adapter::ContractColumnsIterator,
    config::Config,
    l1_execution_data::L1ExecutionData,
    once_transaction_source::OnceTransactionsSource,
    ports::{
        Filter,
        Storage,
        TransactionFiltered,
        TransactionsSource,
    },
    tx_waiter::NoWaitTxs,
};

#[derive(Debug, Clone, Default)]
pub struct ContractsChanges {
    contracts_changes: FxHashMap<ContractId, u64>,
    latest_id: u64,
    changes_storage: FxHashMap<u64, (Vec<ContractId>, Changes)>,
}

impl ContractsChanges {
    pub fn new() -> Self {
        Self {
            contracts_changes: FxHashMap::default(),
            changes_storage: FxHashMap::default(),
            latest_id: 0,
        }
    }

    pub fn add_changes(&mut self, contract_ids: &[ContractId], changes: Changes) {
        let id = self.latest_id;
        self.latest_id += 1;
        for contract_id in contract_ids {
            self.contracts_changes.insert(*contract_id, id);
        }
        self.changes_storage
            .insert(id, (contract_ids.to_vec(), changes));
    }

    pub fn extract_changes(
        &mut self,
        contract_id: &ContractId,
    ) -> Option<(Vec<ContractId>, Changes)> {
        let id = self.contracts_changes.remove(contract_id)?;
        let (contract_ids, changes) = self.changes_storage.remove(&id)?;
        for contract_id in contract_ids.iter() {
            self.contracts_changes.remove(contract_id);
        }
        Some((contract_ids, changes))
    }

    pub fn extract_all_contracts_changes(&mut self) -> Vec<Changes> {
        let mut changes = vec![];
        for id in 0..self.latest_id {
            if let Some((_, change)) = self.changes_storage.remove(&id) {
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

pub struct Scheduler<R, S, PreconfirmationSender> {
    /// Config
    config: Config,
    /// Storage
    pub(crate) storage: S,
    /// Runtime to run the workers
    runtime: Option<Runtime>,
    /// Executor
    pub(crate) executor: BlockExecutor<R, NoWaitTxs, PreconfirmationSender>,
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
    execution_results: FxHashMap<usize, WorkSessionSavedData>,
    /// Blobs transactions to be executed at the end
    blob_transactions: Vec<CheckedTransaction>,
    /// Current scheduler state
    state: SchedulerState,
    /// Total maximum of transactions left
    tx_left: u16,
    /// Total maximum of byte size left
    tx_size_left: u32,
    /// Total remaining gas
    gas_left: u64,
}

struct WorkSessionExecutionResult {
    /// Worker id
    worker_id: usize,
    /// The id of the batch of transactions
    batch_id: usize,
    /// The changes made by the worker used to commit them to the database at the end of execution
    changes: Changes,
    /// The coins created by the worker used to verify the coin dependency chain at the end of execution
    /// We also store the index of the transaction in the batch in case the usage is in the same batch
    coins_created: Vec<CoinInBatch>,
    /// The coins used by the worker used to verify the coin dependency chain at the end of execution
    /// We also store the index of the transaction in the batch in case the creation is in the same batch
    coins_used: Vec<CoinInBatch>,
    /// Contracts used during the execution of the transactions to save the changes for future usage of
    /// the contracts
    contracts_used: Vec<ContractId>,
    /// The transactions that were skipped by the worker
    skipped_tx: Vec<(TxId, ExecutorError)>,
    /// Batch of transactions (included skipped ones) useful to re-execute them in case of fallback skipped
    txs: Vec<Transaction>,
    /// Message ids
    message_ids: Vec<MessageId>,
    /// Events
    events: Vec<Event>,
    /// tx statuses
    tx_statuses: Vec<TransactionExecutionStatus>,
    /// used gas
    used_gas: u64,
    /// used tx size
    used_size: u32,
    /// coinbase
    coinbase: u64,
}

#[derive(Default)]
struct WorkSessionSavedData {
    /// The changes made by the worker used to commit them to the database at the end of execution
    changes: Changes,
    /// The coins created by the worker used to verify the coin dependency chain at the end of execution
    /// We also store the index of the transaction in the batch in case the usage is in the same batch
    coins_created: Vec<CoinInBatch>,
    /// The coins used by the worker used to verify the coin dependency chain at the end of execution
    /// We also store the index of the transaction in the batch in case the creation is in the same batch
    coins_used: Vec<CoinInBatch>,
    /// The transactions of the batch
    txs: Vec<Transaction>,
    /// Message ids
    message_ids: Vec<MessageId>,
    /// events
    events: Vec<Event>,
    /// tx statuses
    tx_statuses: Vec<TransactionExecutionStatus>,
    /// skipped tx
    skipped_tx: Vec<(TxId, ExecutorError)>,
    /// used gas
    used_gas: u64,
    /// used tx size
    used_size: u32,
    /// coinbase
    coinbase: u64,
}

/// Error type for the scheduler
#[derive(Debug, derive_more::Display)]

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

#[derive(Default, Debug)]
pub struct SchedulerExecutionResult {
    pub header: PartialBlockHeader,
    pub transactions: Vec<Transaction>,
    pub events: Vec<Event>,
    pub message_ids: Vec<MessageId>,
    pub skipped_txs: Vec<(TxId, ExecutorError)>,
    pub transactions_status: Vec<TransactionExecutionStatus>,
    pub changes: StorageChanges,
    pub used_gas: u64,
    pub used_size: u32,
    pub coinbase: u64,
}

impl SchedulerExecutionResult {
    pub fn add_blob_execution_data(
        &mut self,
        blob_execution_data: ExecutionData,
        blob_txs: Vec<Transaction>,
    ) {
        self.transactions.extend(blob_txs);
        self.events.extend(blob_execution_data.events);
        self.message_ids.extend(blob_execution_data.message_ids);
        self.skipped_txs
            .extend(blob_execution_data.skipped_transactions);
        self.transactions_status
            .extend(blob_execution_data.tx_status);
        // Should contains all the changes from all executions
        self.changes = StorageChanges::Changes(blob_execution_data.changes);
        self.used_gas = self.used_gas.saturating_add(blob_execution_data.used_gas);
        self.used_size = self.used_size.saturating_add(blob_execution_data.used_size);
        self.coinbase = self.coinbase.saturating_add(blob_execution_data.coinbase);
    }
}

#[derive(Default)]
pub(crate) struct PreparedBatch {
    pub transactions: Vec<CheckedTransaction>,
    pub gas: u64,
    pub blob_transactions: Vec<CheckedTransaction>,
    // Separated from the other gas because this need to be deduced to the global one and not a core one
    pub blob_gas: u64,
    pub total_size: u32,
    pub contracts_used: Vec<ContractId>,
    pub coins_used: Vec<CoinInBatch>,
    pub number_of_transactions: u16,
}

pub struct BlockConstraints {
    pub block_gas_limit: u64,
    pub total_execution_time: Duration,
    pub block_transaction_size_limit: u32,
    pub block_transaction_count_limit: u16,
}

// Shutdown the tokio runtime to avoid panic if executor is already
// used from another tokio runtime
impl<R, S, PreconfirmationSender> Drop for Scheduler<R, S, PreconfirmationSender> {
    fn drop(&mut self) {
        if let Some(runtime) = self.runtime.take() {
            runtime.shutdown_background();
        }
    }
}

impl<R, S, PreconfirmationSender> Scheduler<R, S, PreconfirmationSender> {
    pub fn new(
        config: Config,
        relayer: R,
        storage: S,
        preconfirmation_sender: PreconfirmationSender,
    ) -> Self {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(config.number_of_cores.get())
            .enable_all()
            .build()
            .unwrap();

        let executor = BlockExecutor::new(
            relayer,
            ExecutionOptions {
                forbid_fake_coins: false,
                backtrace: false,
            },
            ConsensusParameters::default(),
            NoWaitTxs,
            preconfirmation_sender,
            true, // dry run
        )
        .unwrap();

        Self {
            runtime: Some(runtime),
            executor,
            storage,
            tx_left: 0,
            tx_size_left: 0,
            gas_left: 0,
            current_available_workers: (0..config.number_of_cores.get()).collect(),
            config,
            current_execution_tasks: FuturesUnordered::new(),
            blob_transactions: vec![],
            execution_results: FxHashMap::default(),
            state: SchedulerState::TransactionsReadyForPickup,
            contracts_changes: ContractsChanges::new(),
            current_executing_contracts: HashSet::new(),
        }
    }

    fn reset(&mut self) {
        self.tx_left = 0;
        self.tx_size_left = 0;
        self.gas_left = 0;
        self.current_available_workers = (0..self.config.number_of_cores.get()).collect();
        self.current_executing_contracts.clear();
        self.execution_results.clear();
        self.contracts_changes.clear();
        self.current_execution_tasks = FuturesUnordered::new();
        self.state = SchedulerState::TransactionsReadyForPickup;
    }
}

impl<R, S, PreconfirmationSender, View> Scheduler<R, S, PreconfirmationSender>
where
    R: RelayerPort + Clone + Send + 'static,
    PreconfirmationSender: PreconfirmationSenderPort + Clone + Send + 'static,
    S: AtomicView<LatestView = View> + Clone + Send + 'static,
    View: Storage + KeyValueInspect<Column = Column> + Send + Sync + 'static,
{
    pub async fn run<TxSource: TransactionsSource>(
        &mut self,
        components: &mut Components<TxSource>,
        da_changes: Changes,
        block_constraints: BlockConstraints,
        l1_execution_data: L1ExecutionData,
    ) -> Result<SchedulerExecutionResult, SchedulerError> {
        let view = self
            .storage
            .latest_view()
            .map_err(SchedulerError::StorageError)?;
        let storage_with_da = Arc::new(view.into_transaction().with_changes(da_changes));
        self.tx_left = block_constraints
            .block_transaction_count_limit
            .checked_sub(l1_execution_data.tx_count)
            .ok_or(SchedulerError::InternalError(
                "Cannot insert more transactions: tx_count full".to_string(),
            ))?;
        self.tx_size_left = block_constraints
            .block_transaction_size_limit
            .checked_sub(l1_execution_data.used_size)
            .ok_or(SchedulerError::InternalError(
                "Cannot insert more transactions: tx_size full".to_string(),
            ))?;

        let consensus_parameters_version =
            components.header_to_produce.consensus_parameters_version;
        let block_height = *components.header_to_produce.height();

        let consensus_parameters = {
            let latest_view = self
                .storage
                .latest_view()
                .map_err(SchedulerError::StorageError)?;

            let consensus_parameters = latest_view
                .get_consensus_parameters(consensus_parameters_version)
                .map_err(SchedulerError::StorageError)?;
            self.executor
                .set_consensus_params(consensus_parameters.clone());
            consensus_parameters
        };

        let new_tx_notifier = components
            .transactions_source
            .get_new_transactions_notifier();
        let now = tokio::time::Instant::now();
        let deadline = now + block_constraints.total_execution_time;
        let mut nb_batch_created = 0;
        let mut nb_transactions = 0;
        let initial_gas_per_worker = block_constraints
            .block_gas_limit
            .checked_sub(l1_execution_data.used_gas)
            .ok_or(SchedulerError::InternalError(
                "L1 transactions consumed all the gas".to_string(),
            ))?
            .checked_div(self.config.number_of_cores.get() as u64)
            .ok_or(SchedulerError::InternalError(
                "Invalid block gas limit".to_string(),
            ))?;

        'outer: loop {
            if self.is_worker_idling() {
                let batch = self.ask_new_transactions_batch(
                    &mut components.transactions_source,
                    now,
                    initial_gas_per_worker,
                    block_constraints.total_execution_time,
                )?;
                let batch_len = batch.number_of_transactions;

                if batch.transactions.is_empty() {
                    self.blob_transactions
                        .extend(batch.blob_transactions.into_iter());
                    continue 'outer;
                }

                self.execute_batch(
                    consensus_parameters_version,
                    components,
                    batch,
                    nb_batch_created,
                    nb_transactions,
                    storage_with_da.clone(),
                )?;

                nb_batch_created += 1;
                nb_transactions += batch_len;
            } else if self.current_execution_tasks.is_empty() {
                tokio::select! {
                    _ = new_tx_notifier.notified() => {
                        self.new_executable_transactions();
                    }
                    _ = tokio::time::sleep_until(deadline) => {
                        break 'outer;
                    }
                }
            } else {
                tokio::select! {
                    _ = new_tx_notifier.notified() => {
                        self.new_executable_transactions();
                    }
                    result = self.current_execution_tasks.select_next_some() => {
                        match result {
                            Ok(res) => {
                                if !res.skipped_tx.is_empty() {
                                    self.sequential_fallback(block_height, &consensus_parameters, res.batch_id, res.txs, res.coins_used, res.coins_created).await?;
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

        self.wait_all_execution_tasks(
            block_height,
            &consensus_parameters,
            block_constraints.total_execution_time,
        )
        .await?;

        let mut res = self.verify_coherency_and_merge_results(
            nb_batch_created,
            components.header_to_produce,
            l1_execution_data,
        )?;

        if self.blob_transactions.is_empty() {
            let mut merged: Changes = Changes::default();
            for changes in res.changes.extract_list_of_changes() {
                merged.extend(changes);
            }
            let storage_with_res = self
                .storage
                .latest_view()
                .map_err(SchedulerError::StorageError)?
                .into_transaction()
                .with_changes(merged);
            let (blob_execution_data, blob_txs) = self
                .execute_blob_transactions(
                    components,
                    storage_with_res,
                    nb_transactions,
                    consensus_parameters_version,
                )
                .await?;
            res.add_blob_execution_data(blob_execution_data, blob_txs);
        }

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

    fn ask_new_transactions_batch<TxSource: TransactionsSource>(
        &mut self,
        tx_source: &mut TxSource,
        start_execution_time: tokio::time::Instant,
        initial_gas: u64,
        total_execution_time: Duration,
    ) -> Result<PreparedBatch, SchedulerError> {
        let spent_time = start_execution_time.elapsed();
        // Time left in percentage to have the gas percentage left
        // TODO: Maybe avoid as u32
        let current_gas = std::cmp::min(
            initial_gas
                .saturating_mul(
                    (total_execution_time.as_millis() as u64)
                        .saturating_sub(spent_time.as_millis() as u64),
                )
                .saturating_div(total_execution_time.as_millis() as u64),
            self.gas_left
                .saturating_div(self.config.number_of_cores.get() as u64),
        );

        let (batch, filtered, filter) = tx_source.get_executable_transactions(
            current_gas,
            self.tx_left,
            self.tx_size_left,
            Filter {
                excluded_contract_ids: std::mem::take(
                    &mut self.current_executing_contracts,
                ),
            },
        );
        self.current_executing_contracts = filter.excluded_contract_ids;

        if batch.is_empty() {
            if filtered == TransactionFiltered::Filtered {
                self.state = SchedulerState::WaitingForWorker;
            } else {
                self.state = SchedulerState::WaitingForNewTransaction;
            }
        }

        let prepared_batch = prepare_transactions_batch(batch)?;
        self.tx_size_left = self.tx_size_left.saturating_sub(prepared_batch.total_size);
        self.gas_left = self
            .gas_left
            .saturating_sub(
                prepared_batch
                    .gas
                    .saturating_div(self.config.number_of_cores.get() as u64),
            )
            .saturating_sub(prepared_batch.blob_gas);
        self.tx_left = self
            .tx_left
            .saturating_sub(prepared_batch.number_of_transactions);
        Ok(prepared_batch)
    }

    fn execute_batch<TxSource>(
        &mut self,
        consensus_parameters_version: u32,
        components: &Components<TxSource>,
        mut batch: PreparedBatch,
        batch_id: usize,
        start_idx_txs: u16,
        storage_with_da: Arc<StorageTransaction<View>>,
    ) -> Result<(), SchedulerError> {
        let worker_id = self.current_available_workers.pop_front().ok_or(
            SchedulerError::InternalError("No available workers".to_string()),
        )?;
        let runtime = self.runtime.as_ref().unwrap();

        let mut required_changes: Changes = Changes::default();
        let mut new_contracts_used = vec![];
        for contract in batch.contracts_used.iter() {
            if let Some((contract_ids, changes)) =
                self.contracts_changes.extract_changes(contract)
            {
                self.current_executing_contracts
                    .extend(contract_ids.clone());
                new_contracts_used.extend(contract_ids);
                required_changes.extend(changes);
            }
        }
        batch.contracts_used.extend(new_contracts_used);

        let executor = self.executor.clone();
        let coinbase_recipient = components.coinbase_recipient;
        let gas_price = components.gas_price;
        let header_to_produce = components.header_to_produce;
        self.current_execution_tasks.push(runtime.spawn({
            let storage_with_da = storage_with_da.clone();
            async move {
                let mut execution_data = ExecutionData::default();
                execution_data.tx_count = start_idx_txs;
                let storage_tx = storage_with_da
                    .into_transaction()
                    .with_changes(required_changes);
                // TODO: Error management
                let block = executor
                    .execute_l2_transactions(
                        Components {
                            header_to_produce,
                            transactions_source: OnceTransactionsSource::new(
                                batch.transactions,
                                consensus_parameters_version,
                            ),
                            coinbase_recipient,
                            gas_price,
                        },
                        storage_tx,
                        &mut execution_data,
                    )
                    .await
                    .unwrap();
                // TODO: Outputs seems to not be resolved here, why? It should be the case need to investigate
                let coins_created = get_coins_outputs(
                    block.transactions.iter().zip(
                        execution_data
                            .tx_status
                            .iter()
                            .map(|tx_status| tx_status.id),
                    ),
                );
                if !execution_data.skipped_transactions.is_empty() {
                    for (tx_id, error) in execution_data.skipped_transactions.iter() {
                        batch.coins_used.retain(|coin| {
                            if coin.tx_id == *tx_id {
                                tracing::warn!("Transaction {tx_id} skipped: {error}");
                                false
                            } else {
                                true
                            }
                        });
                    }
                }
                WorkSessionExecutionResult {
                    worker_id,
                    batch_id,
                    changes: execution_data.changes,
                    coins_created,
                    coins_used: batch.coins_used,
                    contracts_used: batch.contracts_used,
                    skipped_tx: execution_data.skipped_transactions,
                    txs: block.transactions,
                    message_ids: execution_data.message_ids,
                    events: execution_data.events,
                    tx_statuses: execution_data.tx_status,
                    used_gas: execution_data.used_gas,
                    used_size: execution_data.used_size,
                    coinbase: execution_data.coinbase,
                }
            }
        }));
        self.blob_transactions.extend(batch.blob_transactions);
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
        let mut tmp_contracts_changes = HashMap::default();
        for column in ContractColumnsIterator::new() {
            let column = column.as_u32();
            if let Some(changes) = res.changes.remove(&column) {
                tmp_contracts_changes.insert(column, changes);
            }
        }
        self.contracts_changes
            .add_changes(res.contracts_used.as_ref(), tmp_contracts_changes);
        self.execution_results.insert(
            res.batch_id,
            WorkSessionSavedData {
                changes: res.changes,
                coins_created: res.coins_created,
                coins_used: res.coins_used,
                txs: res.txs,
                message_ids: res.message_ids,
                events: res.events,
                tx_statuses: res.tx_statuses,
                skipped_tx: res.skipped_tx,
                used_gas: res.used_gas,
                used_size: res.used_size,
                coinbase: res.coinbase,
            },
        );
        self.current_available_workers.push_back(res.worker_id);
    }

    async fn wait_all_execution_tasks(
        &mut self,
        block_height: BlockHeight,
        consensus_params: &ConsensusParameters,
        total_execution_time: Duration,
    ) -> Result<(), SchedulerError> {
        let tolerance_execution_time_overflow = total_execution_time / 10;
        let now = tokio::time::Instant::now();

        // We have reached the deadline
        // We need to merge the states of all the workers
        while !self.current_execution_tasks.is_empty() {
            match self.current_execution_tasks.next().await {
                Some(Ok(res)) => {
                    if !res.skipped_tx.is_empty() {
                        self.sequential_fallback(
                            block_height,
                            consensus_params,
                            res.batch_id,
                            res.txs,
                            res.coins_used,
                            res.coins_created,
                        )
                        .await?;
                        break;
                    } else {
                        self.execution_results.insert(
                            res.batch_id,
                            WorkSessionSavedData {
                                changes: res.changes,
                                coins_created: res.coins_created,
                                coins_used: res.coins_used,
                                txs: res.txs,
                                message_ids: res.message_ids,
                                events: res.events,
                                tx_statuses: res.tx_statuses,
                                skipped_tx: res.skipped_tx,
                                used_gas: res.used_gas,
                                used_size: res.used_size,
                                coinbase: res.coinbase,
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
        partial_block_header: PartialBlockHeader,
        l1_execution_data: L1ExecutionData,
    ) -> Result<SchedulerExecutionResult, SchedulerError> {
        let L1ExecutionData {
            coinbase,
            used_gas,
            used_size,
            message_ids,
            transactions_status,
            events,
            skipped_txs,
            ..
        } = l1_execution_data;
        let mut exec_result = SchedulerExecutionResult {
            header: partial_block_header,
            transactions: vec![],
            events,
            message_ids,
            skipped_txs,
            transactions_status,
            changes: StorageChanges::default(),
            used_gas,
            used_size,
            coinbase,
        };

        let mut storage_changes = vec![];
        let latest_view = self
            .storage
            .latest_view()
            .map_err(SchedulerError::StorageError)?;
        let mut compiled_created_coins = CoinDependencyChainVerifier::new();
        for batch_id in 0..nb_batch {
            if let Some(changes) = self.execution_results.remove(&batch_id) {
                compiled_created_coins
                    .register_coins_created(batch_id, changes.coins_created);
                compiled_created_coins.verify_coins_used(
                    batch_id,
                    changes.coins_used.iter(),
                    &latest_view,
                )?;
                storage_changes.push(changes.changes);
                exec_result.events.extend(changes.events);
                exec_result.message_ids.extend(changes.message_ids);
                exec_result.skipped_txs.extend(changes.skipped_tx);
                exec_result.transactions_status.extend(changes.tx_statuses);
                exec_result.transactions.extend(changes.txs);
                exec_result.used_gas = exec_result
                    .used_gas
                    .checked_add(changes.used_gas)
                    .ok_or_else(|| {
                        SchedulerError::InternalError(
                            "used gas has overflowed u64".to_string(),
                        )
                    })?;
                exec_result.used_size = exec_result
                    .used_size
                    .checked_add(changes.used_size)
                    .ok_or_else(|| {
                        SchedulerError::InternalError(
                            "used size has overflowed u32".to_string(),
                        )
                    })?;
                exec_result.coinbase = exec_result
                    .coinbase
                    .checked_add(changes.coinbase)
                    .ok_or_else(|| {
                        SchedulerError::InternalError(
                            "coinbase has overflowed u64".to_string(),
                        )
                    })?;
            } else {
                return Err(SchedulerError::InternalError(format!(
                    "Batch {batch_id} not found in the execution results"
                )));
            }
        }
        exec_result.header = partial_block_header;
        storage_changes.extend(self.contracts_changes.extract_all_contracts_changes());
        exec_result.changes = StorageChanges::ChangesList(storage_changes);
        Ok(exec_result)
    }

    async fn execute_blob_transactions<TxSource>(
        &mut self,
        components: &Components<TxSource>,
        storage: StorageTransaction<View>,
        start_idx_txs: u16,
        consensus_parameters_version: u32,
    ) -> Result<(ExecutionData, Vec<Transaction>), SchedulerError> {
        let mut execution_data = ExecutionData::default();
        execution_data.tx_count = start_idx_txs;
        let block = self
            .executor
            .clone()
            .execute_l2_transactions(
                Components {
                    header_to_produce: components.header_to_produce,
                    transactions_source: OnceTransactionsSource::new(
                        std::mem::take(&mut self.blob_transactions),
                        consensus_parameters_version,
                    ),
                    coinbase_recipient: components.coinbase_recipient,
                    gas_price: components.gas_price,
                },
                storage,
                &mut execution_data,
            )
            .await
            .map_err(SchedulerError::ExecutionError)?;
        Ok((execution_data, block.transactions))
    }

    // Wait for all the workers to finish gather all theirs transactions
    // re-execute them in one worker without skipped one. We also need to
    // fetch all the possible executed and stored batch after the lowest batch_id we gonna
    // re-execute.
    // Tell the TransactionSource that this transaction is skipped
    // to avoid sending new transactions that depend on it (using preconfirmation squeeze out)
    //
    // Can be replaced by a mechanism that replace the skipped_tx by a dummy transaction to not shift everything
    async fn sequential_fallback(
        &mut self,
        block_height: BlockHeight,
        consensus_params: &ConsensusParameters,
        batch_id: usize,
        mut txs: Vec<Transaction>,
        mut coins_used: Vec<CoinInBatch>,
        mut coins_created: Vec<CoinInBatch>,
    ) -> Result<(), SchedulerError> {
        let current_execution_tasks = std::mem::take(&mut self.current_execution_tasks);
        let mut lower_batch_id = batch_id;
        let mut higher_batch_id = batch_id;
        let mut all_txs_by_batch_id = FxHashMap::default();
        for future in current_execution_tasks {
            match future.await {
                Ok(res) => {
                    all_txs_by_batch_id.insert(
                        res.batch_id,
                        (res.txs, res.coins_created, res.coins_used),
                    );
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

        let mut all_txs: Vec<CheckedTransaction> = vec![];
        let mut all_coins_created: Vec<CoinInBatch> = vec![];
        let mut all_coins_used: Vec<CoinInBatch> = vec![];
        for id in lower_batch_id..higher_batch_id {
            if let Some((txs, coins_created, coins_used)) =
                all_txs_by_batch_id.remove(&id)
            {
                for tx in txs {
                    all_txs.push(
                        tx.into_checked(block_height, consensus_params)
                            .map_err(|e| {
                                SchedulerError::InternalError(format!(
                                    "Failed to convert transaction to checked: {e:?}"
                                ))
                            })?
                            .into(),
                    );
                }
                all_coins_created.extend(coins_created);
                all_coins_used.extend(coins_used);
            } else if let Some(res) = self.execution_results.remove(&id) {
                for tx in res.txs {
                    all_txs.push(
                        tx.into_checked(block_height, consensus_params)
                            .map_err(|e| {
                                SchedulerError::InternalError(format!(
                                    "Failed to convert transaction to checked: {e:?}"
                                ))
                            })?
                            .into(),
                    );
                }
                all_coins_created.extend(res.coins_created);
                all_coins_used.extend(res.coins_used);
            } else if id == batch_id {
                // Ordering of transactions is important so we need to place this code here
                // which avoid to just move, but it's fine because we should only trigger this once
                let txs = std::mem::take(&mut txs);
                for tx in txs {
                    all_txs.push(
                        tx.into_checked(block_height, consensus_params)
                            .map_err(|e| {
                                SchedulerError::InternalError(format!(
                                    "Failed to convert transaction to checked: {e:?}"
                                ))
                            })?
                            .into(),
                    );
                }
                all_coins_created.extend(std::mem::take(&mut coins_created));
                all_coins_used.extend(std::mem::take(&mut coins_used));
            } else {
                tracing::error!("Batch {id} not found in the execution results");
            }
        }

        let (block, execution_data) = self
            .executor
            .clone()
            .execute(
                Components {
                    header_to_produce: PartialBlockHeader::default(),
                    transactions_source: OnceTransactionsSource::new(all_txs, 0),
                    coinbase_recipient: Default::default(),
                    gas_price: Default::default(),
                },
                self.storage.latest_view().unwrap().write_transaction(),
            )
            .await
            .map_err(SchedulerError::ExecutionError)?;

        // Save execution results for all batch id with empty data
        // to not break the batch chain
        for id in lower_batch_id..higher_batch_id {
            self.execution_results
                .insert(id, WorkSessionSavedData::default());
        }
        // Save the execution results for the current batch
        self.execution_results.insert(
            batch_id,
            WorkSessionSavedData {
                changes: execution_data.changes,
                coins_created: all_coins_created,
                coins_used: all_coins_used,
                txs: block.transactions,
                message_ids: execution_data.message_ids,
                events: execution_data.events,
                tx_statuses: execution_data.tx_status,
                skipped_tx: vec![],
                used_gas: execution_data.used_gas,
                used_size: execution_data.used_size,
                coinbase: execution_data.coinbase,
            },
        );
        Ok(())
    }
}

struct CoinDependencyChainVerifier {
    coins_registered: FxHashMap<UtxoId, (usize, CoinInBatch)>,
}

impl CoinDependencyChainVerifier {
    fn new() -> Self {
        Self {
            coins_registered: FxHashMap::default(),
        }
    }

    fn register_coins_created(
        &mut self,
        batch_id: usize,
        coins_created: Vec<CoinInBatch>,
    ) {
        for coin in coins_created {
            self.coins_registered.insert(*coin.utxo(), (batch_id, coin));
        }
    }

    fn verify_coins_used<'a, S>(
        &self,
        batch_id: usize,
        coins_used: impl Iterator<Item = &'a CoinInBatch>,
        storage: &S,
    ) -> Result<(), SchedulerError>
    where
        S: Storage + Send,
    {
        for coin in coins_used {
            match storage.get_coin(coin.utxo()) {
                Ok(Some(db_coin)) => {
                    // Coin is in the database
                    match coin.equal_compressed_coin(&db_coin) {
                        true => continue,
                        false => {
                            return Err(SchedulerError::InternalError(format!(
                                "coin is invalid: {}",
                                coin.utxo(),
                            )));
                        }
                    }
                }
                Ok(None) => {
                    // Coin is not in the database
                    match self.coins_registered.get(coin.utxo()) {
                        Some((coin_creation_batch_id, registered_coin)) => {
                            // Coin is in the block
                            if coin_creation_batch_id <= &batch_id
                                && registered_coin.idx() <= coin.idx()
                                && registered_coin == coin
                            {
                                // Coin is created in a batch that is before the current one
                                continue;
                            } else {
                                // Coin is created in a batch that is after the current one
                                return Err(SchedulerError::InternalError(format!(
                                    "Coin {} is created in a batch that is after the current one",
                                    coin.utxo()
                                )));
                            }
                        }
                        None => {
                            return Err(SchedulerError::InternalError(format!(
                                "Coin {} is not in the database and not created in the block",
                                coin.utxo(),
                            )));
                        }
                    }
                }
                Err(e) => {
                    return Err(SchedulerError::InternalError(format!(
                        "Error while getting coin {}: {e}",
                        coin.utxo(),
                    )));
                }
            }
        }
        Ok(())
    }
}

#[allow(clippy::type_complexity)]
fn prepare_transactions_batch(
    batch: Vec<CheckedTransaction>,
) -> Result<PreparedBatch, SchedulerError> {
    let mut prepared_batch = PreparedBatch::default();

    for (idx, tx) in batch.into_iter().enumerate() {
        let tx_id = tx.id();
        let inputs = tx.inputs();
        for input in inputs.iter() {
            match input {
                fuel_core_types::fuel_tx::Input::Contract(contract) => {
                    prepared_batch.contracts_used.push(contract.contract_id);
                }
                fuel_core_types::fuel_tx::Input::CoinSigned(coin) => {
                    prepared_batch
                        .coins_used
                        .push(CoinInBatch::from_signed_coin(coin, idx, tx_id));
                }
                fuel_core_types::fuel_tx::Input::CoinPredicate(coin) => {
                    prepared_batch
                        .coins_used
                        .push(CoinInBatch::from_predicate_coin(coin, idx, tx_id));
                }
                _ => {}
            }
        }

        for output in tx.outputs().iter() {
            match output {
                Output::ContractCreated { contract_id, .. } => {
                    prepared_batch.contracts_used.push(*contract_id);
                }
                _ => {}
            }
        }

        let is_blob = matches!(&tx, CheckedTransaction::Blob(_));
        prepared_batch.total_size += tx.size() as u32;
        prepared_batch.number_of_transactions += 1;
        if is_blob {
            prepared_batch.blob_gas += CheckedTransactionExt::max_gas(&tx)?;
            prepared_batch.blob_transactions.push(tx);
        } else {
            prepared_batch.gas += CheckedTransactionExt::max_gas(&tx)?;
            prepared_batch.transactions.push(tx);
        }
    }
    Ok(prepared_batch)
}

fn get_coins_outputs<'a>(
    transactions: impl Iterator<Item = (&'a Transaction, TxId)>,
) -> Vec<CoinInBatch> {
    let mut coins = vec![];
    for (idx, (tx, tx_id)) in transactions.enumerate() {
        for output in tx.outputs().iter() {
            match output {
                Output::Coin {
                    to,
                    amount,
                    asset_id,
                } => {
                    coins.push(CoinInBatch {
                        utxo_id: UtxoId::new(tx_id, idx as u16),
                        idx,
                        tx_id,
                        owner: *to,
                        amount: *amount,
                        asset_id: *asset_id,
                    });
                }
                Output::Change {
                    to,
                    amount,
                    asset_id,
                } => {
                    coins.push(CoinInBatch {
                        utxo_id: UtxoId::new(tx_id, idx as u16),
                        idx,
                        tx_id,
                        owner: *to,
                        amount: *amount,
                        asset_id: *asset_id,
                    });
                }
                Output::Variable {
                    to,
                    amount,
                    asset_id,
                } => {
                    coins.push(CoinInBatch {
                        utxo_id: UtxoId::new(tx_id, idx as u16),
                        idx,
                        tx_id,
                        owner: *to,
                        amount: *amount,
                        asset_id: *asset_id,
                    });
                }
                _ => {}
            }
        }
    }
    coins
}
