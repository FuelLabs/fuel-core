use std::{
    collections::HashSet,
    sync::Arc,
    time::Duration,
};

use crate::{
    config::Config,
    in_memory_transaction_with_contracts::InMemoryTransactionWithContracts,
    l1_execution_data::L1ExecutionData,
    memory::MemoryPool,
    once_transaction_source::OnceTransactionsSource,
    ports::{
        Filter,
        TransactionsSource,
    },
    scheduler::workers::{
        WorkerId,
        WorkerPool,
    },
    tx_waiter::NoWaitTxs,
};
use ::futures::{
    StreamExt,
    stream::FuturesUnordered,
};
use coin::{
    CoinDependencyChainVerifier,
    CoinInBatch,
};
use fuel_core_executor::{
    executor::{
        BlockExecutor,
        ExecutionData,
    },
    ports::{
        MaybeCheckedTransaction,
        PreconfirmationSenderPort,
        RelayerPort,
    },
};
use fuel_core_storage::{
    Error as StorageError,
    column::Column,
    kv_store::KeyValueInspect,
    structured_storage::StructuredStorage,
    transactional::{
        AtomicView,
        Changes,
        ConflictPolicy,
        IntoTransaction,
        Modifiable,
        ReadTransaction,
        StorageChanges,
        StorageTransaction,
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
    fuel_types::Nonce,
    fuel_vm::checked_transaction::IntoChecked,
    services::{
        block_producer::Components,
        executor::{
            Error as ExecutorError,
            Event,
            TransactionExecutionStatus,
        },
    },
};
use futures::future::Either;
use fxhash::FxHashMap;
use tokio::{
    runtime::Runtime,
    time::Instant,
};

mod coin;
mod workers;

pub struct Scheduler<'a, R, S, PreconfirmationSender> {
    /// The partial block header of the future block without transactions related information.
    header_to_produce: PartialBlockHeader,
    /// The `ContractId` of the fee recipient.
    coinbase_recipient: ContractId,
    /// The gas price for all transactions in the block.
    gas_price: u64,
    /// Config
    config: Config,
    /// Storage
    pub(crate) storage: S,
    /// Executor to execute the transactions
    executor: BlockExecutor<R, NoWaitTxs, PreconfirmationSender>,
    /// Consensus parameters
    consensus_parameters: ConsensusParameters,
    /// Runtime to run the workers
    runtime: &'a Runtime,
    /// List of available workers
    worker_pool: WorkerPool,
    /// Memory pool to store the memory instances
    memory_pool: MemoryPool,
    /// All contracts changes
    contracts_changes: FxHashMap<ContractId, Changes>,
    /// Current contracts being executed
    current_executing_contracts: HashSet<ContractId>,
    /// Current execution tasks
    current_execution_tasks: FuturesUnordered<
        tokio::task::JoinHandle<Result<WorkSessionExecutionResult, ExecutorError>>,
    >,
    // All executed transactions batch associated with their id
    execution_results: FxHashMap<usize, WorkSessionSavedData>,
    /// Blobs transactions to be executed at the end
    blob_transactions: Vec<MaybeCheckedTransaction>,
    /// Current scheduler state
    state: SchedulerState,
    /// Total maximum of transactions left
    tx_left: u32,
    /// Total maximum of byte size left
    tx_size_left: u64,
    /// Total remaining gas
    gas_left: u64,
    /// Deadline for the block production
    deadline: Instant,
    /// Gas used by blob transactions
    blob_gas: u64,
}

struct WorkSessionExecutionResult {
    /// Worker id
    worker_id: WorkerId,
    /// The id of the batch of transactions
    batch_id: usize,
    /// The changes made by the worker used to commit them to the database at the end of execution.
    /// It excludes contract changes.
    changes: Changes,
    /// The changes made by the worker per contract.
    changes_per_contract: FxHashMap<ContractId, Changes>,
    /// The coins created by the worker used to verify the coin dependency chain at the end of execution
    /// We also store the index of the transaction in the batch in case the usage is in the same batch
    coins_created: Vec<CoinInBatch>,
    /// The coins used by the worker used to verify the coin dependency chain at the end of execution
    /// We also store the index of the transaction in the batch in case the creation is in the same batch
    coins_used: Vec<CoinInBatch>,
    /// Messages nonces used, useful to check double spending
    message_nonces_used: Vec<Nonce>,
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
    /// Difference between gas expected and gas used by the transactions
    gas_diff: u64,
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
    /// Messages nonces used, useful to check double spending
    message_nonces_used: Vec<Nonce>,
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

impl From<StorageError> for SchedulerError {
    fn from(error: StorageError) -> Self {
        SchedulerError::StorageError(error)
    }
}

impl From<ExecutorError> for SchedulerError {
    fn from(error: ExecutorError) -> Self {
        SchedulerError::ExecutionError(error)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SchedulerState {
    /// Ready for a new worker to get some transactions
    TransactionsReadyForPickup,
    /// There no transactions available for the execution.
    NoTransactionsForPickup,
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
        debug_assert!(
            self.changes.is_empty(),
            "Changes should be empty after blob merging"
        );
        self.used_gas = self.used_gas.saturating_add(blob_execution_data.used_gas);
        self.used_size = self.used_size.saturating_add(blob_execution_data.used_size);
        self.coinbase = self.coinbase.saturating_add(blob_execution_data.coinbase);
    }
}

#[derive(Default)]
pub(crate) struct PreparedBatch {
    pub transactions: Vec<MaybeCheckedTransaction>,
    pub gas: u64,
    pub blob_transactions: Vec<MaybeCheckedTransaction>,
    // Separated from the other gas because this need to be deduced to the global one and not a core one
    pub blob_gas: u64,
    pub total_size: u64,
    pub contracts_used: Vec<ContractId>,
    pub coins_used: Vec<CoinInBatch>,
    pub message_nonces_used: Vec<Nonce>,
    pub number_of_transactions: u32,
}

pub struct BlockConstraints {
    pub block_gas_limit: u64,
    pub total_execution_time: Duration,
    pub block_transaction_size_limit: u32,
    pub block_transaction_count_limit: u16,
}

impl<'a, R, S, PreconfirmationSender> Scheduler<'a, R, S, PreconfirmationSender> {
    #[allow(clippy::too_many_arguments)]
    pub fn new<TxSource>(
        components: &Components<TxSource>,
        config: Config,
        storage: S,
        executor: BlockExecutor<R, NoWaitTxs, PreconfirmationSender>,
        runtime: &'a Runtime,
        memory_pool: MemoryPool,
        consensus_parameters: ConsensusParameters,
        deadline: Instant,
    ) -> Result<Self, SchedulerError> {
        Ok(Self {
            header_to_produce: components.header_to_produce,
            coinbase_recipient: components.coinbase_recipient,
            gas_price: components.gas_price,
            runtime,
            executor,
            storage,
            // TODO: Use consensus parameters after https://github.com/FuelLabs/fuel-vm/pull/905 is merged
            tx_left: u32::MAX,
            tx_size_left: consensus_parameters.block_transaction_size_limit(),
            gas_left: consensus_parameters.block_gas_limit(),
            worker_pool: WorkerPool::new(config.number_of_cores.get()),
            memory_pool,
            config,
            current_execution_tasks: FuturesUnordered::new(),
            blob_transactions: vec![],
            execution_results: FxHashMap::default(),
            state: SchedulerState::TransactionsReadyForPickup,
            contracts_changes: Default::default(),
            current_executing_contracts: HashSet::new(),
            consensus_parameters,
            blob_gas: 0,
            deadline,
        })
    }
}

impl<'a, R, S, PreconfirmationSender, View> Scheduler<'a, R, S, PreconfirmationSender>
where
    R: RelayerPort + Clone + Send + 'static,
    PreconfirmationSender: PreconfirmationSenderPort + Clone + Send + 'static,
    S: AtomicView<LatestView = View> + Clone + Send + 'static,
    View: KeyValueInspect<Column = Column> + Send + Sync + 'static,
{
    pub async fn run<TxSource>(
        mut self,
        tx_source: &TxSource,
        da_changes: Changes,
        l1_execution_data: L1ExecutionData,
    ) -> Result<SchedulerExecutionResult, SchedulerError>
    where
        TxSource: TransactionsSource,
    {
        let view = self.storage.latest_view()?;
        let storage_with_da = Arc::new(view.into_transaction().with_changes(da_changes));
        self.update_constraints(
            l1_execution_data.tx_count,
            l1_execution_data.used_size as u64,
            l1_execution_data.used_gas,
        )?;

        let mut new_tx_notifier = tx_source.get_new_transactions_notifier();
        let now = Instant::now();
        let deadline = self.deadline;
        let mut nb_batch_created = 0;
        let mut nb_transactions: u32 = l1_execution_data.tx_count;
        let initial_gas_per_worker = self
            .consensus_parameters
            .block_gas_limit()
            .checked_div(self.config.number_of_cores.get() as u64)
            .ok_or(SchedulerError::InternalError(
                "Invalid block gas limit".to_string(),
            ))?
            .checked_sub(l1_execution_data.used_gas)
            .ok_or(SchedulerError::InternalError(
                "L1 transactions consumed all the gas".to_string(),
            ))?;

        'outer: loop {
            let tx_notifier = if new_tx_notifier.has_changed().is_ok() {
                Either::Left(new_tx_notifier.changed())
            } else {
                // If the notifier is closed, we never get new transactions
                Either::Right(futures::future::pending())
            };

            if self.is_worker_idling() {
                // If we requested transactions, we shouldn't drop them,
                // so we await them here.
                let mut batch = self
                    .ask_new_transactions_batch(tx_source, now, initial_gas_per_worker)
                    .await?;

                let blob_transactions = core::mem::take(&mut batch.blob_transactions);
                self.blob_transactions.extend(blob_transactions.into_iter());
                self.blob_gas = self.blob_gas.saturating_add(batch.blob_gas);

                if batch.transactions.is_empty() {
                    self.state = SchedulerState::NoTransactionsForPickup;
                    tracing::warn!(
                        "No transactions to execute, waiting for new transactions or workers to finish"
                    );
                    continue 'outer;
                }

                let batch_len = batch.number_of_transactions;

                self.execute_batch(
                    batch,
                    nb_batch_created,
                    nb_transactions,
                    storage_with_da.clone(),
                )?;

                nb_batch_created = nb_batch_created.saturating_add(1);
                nb_transactions = nb_transactions.checked_add(batch_len).ok_or(
                    SchedulerError::InternalError(
                        "Transaction count overflow".to_string(),
                    ),
                )?;
            } else if self.current_execution_tasks.is_empty() {
                tokio::select! {
                    _ = tx_notifier => {
                        self.state = SchedulerState::TransactionsReadyForPickup;
                    }
                    _ = tokio::time::sleep_until(deadline) => {
                        break 'outer;
                    }
                }
            } else {
                tokio::select! {
                    _ = tx_notifier => {
                        self.state = SchedulerState::TransactionsReadyForPickup;
                    }
                    result = self.current_execution_tasks.select_next_some() => {
                        match result {
                            Ok(res) => {
                                let res = res?;
                                if !res.skipped_tx.is_empty() {
                                    drop(res.worker_id);
                                    self.sequential_fallback(res.batch_id, res.txs, res.coins_used, res.coins_created, res.message_nonces_used).await?;
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

        let mut res = self.verify_coherency_and_merge_results(
            nb_batch_created,
            l1_execution_data,
            storage_with_da.clone(),
        )?;

        if !self.blob_transactions.is_empty() {
            let mut tx = StorageTransaction::transaction(
                storage_with_da.clone(),
                ConflictPolicy::Fail,
                Default::default(),
            );

            // TODO: Rework this part to avoid `commit_changes` and decreasing performance
            for changes in res.changes.extract_list_of_changes() {
                if let Err(e) = tx.commit_changes(changes) {
                    return Err(SchedulerError::StorageError(e));
                }
            }
            res.changes = StorageChanges::Changes(Default::default());

            let (blob_execution_data, blob_txs) =
                self.execute_blob_transactions(tx, nb_transactions).await?;
            res.add_blob_execution_data(blob_execution_data, blob_txs);
        }

        // TODO: Avoid cloning the DA changes
        let da_changes = storage_with_da.changes().clone();
        match &mut res.changes {
            StorageChanges::Changes(changes) => {
                let changes = core::mem::take(changes);
                res.changes = StorageChanges::ChangesList(vec![changes, da_changes]);
            }
            StorageChanges::ChangesList(list) => {
                list.push(da_changes);
            }
        }

        Ok(res)
    }

    fn update_constraints(
        &mut self,
        tx_number_to_add: u32,
        tx_size_to_add: u64,
        gas_to_add: u64,
    ) -> Result<(), SchedulerError> {
        self.tx_left = self.tx_left.checked_sub(tx_number_to_add).ok_or(
            SchedulerError::InternalError(
                "Cannot add more transactions: tx_left underflow".to_string(),
            ),
        )?;
        self.tx_size_left = self.tx_size_left.checked_sub(tx_size_to_add).ok_or(
            SchedulerError::InternalError(
                "Cannot add more transactions: tx_size_left underflow".to_string(),
            ),
        )?;
        self.gas_left = self.gas_left.checked_sub(gas_to_add).ok_or(
            SchedulerError::InternalError(
                "Cannot add more transactions: gas_left underflow".to_string(),
            ),
        )?;
        Ok(())
    }

    fn is_worker_idling(&self) -> bool {
        !self.worker_pool.is_empty()
            && self.state == SchedulerState::TransactionsReadyForPickup
    }

    async fn ask_new_transactions_batch<TxSource>(
        &mut self,
        tx_source: &TxSource,
        start_execution_time: Instant,
        initial_gas_per_core: u64,
    ) -> Result<PreparedBatch, SchedulerError>
    where
        TxSource: TransactionsSource,
    {
        let total_execution_time = self
            .deadline
            .checked_duration_since(start_execution_time)
            .unwrap_or(Duration::from_millis(1));
        let spent_time = start_execution_time.elapsed();
        let scaled_gas_per_core = (initial_gas_per_core as u128)
            .saturating_mul(
                total_execution_time
                    .as_millis()
                    .saturating_sub(spent_time.as_millis()),
            )
            .checked_div(total_execution_time.as_millis())
            .unwrap_or(initial_gas_per_core as u128);
        let scaled_gas_left = self.gas_left as u128;
        let current_gas = u64::try_from(std::cmp::min(
            scaled_gas_per_core.saturating_sub(self.blob_gas as u128),
            scaled_gas_left.saturating_sub(self.blob_gas as u128),
        ))
        .map_err(|_| {
            SchedulerError::InternalError("Current gas overflowed u64".to_string())
        })?;

        let executable_transactions = tx_source
            .get_executable_transactions(
                current_gas,
                self.tx_left,
                self.tx_size_left,
                Filter {
                    excluded_contract_ids: std::mem::take(
                        &mut self.current_executing_contracts,
                    ),
                },
            )
            .await
            .map_err(|e| {
                SchedulerError::TransactionSourceError(format!(
                    "Failed to get executable transactions: {}",
                    e
                ))
            })?;
        self.current_executing_contracts =
            executable_transactions.filter.excluded_contract_ids;

        let prepared_batch = prepare_transactions_batch(
            &self.consensus_parameters,
            executable_transactions.transactions,
        )?;
        self.update_constraints(
            prepared_batch.number_of_transactions,
            prepared_batch.total_size,
            prepared_batch.gas,
        )?;
        Ok(prepared_batch)
    }

    fn execute_batch(
        &mut self,
        mut batch: PreparedBatch,
        batch_id: usize,
        start_idx_txs: u32,
        storage_with_da: Arc<StorageTransaction<View>>,
    ) -> Result<(), SchedulerError> {
        let worker_id =
            self.worker_pool
                .take_worker()
                .ok_or(SchedulerError::InternalError(
                    "No available workers".to_string(),
                ))?;

        let mut changes_per_contract = Vec::with_capacity(batch.contracts_used.len());

        for contract in batch.contracts_used.iter() {
            self.current_executing_contracts.insert(*contract);
            if let Some(changes) = self.contracts_changes.remove(contract) {
                changes_per_contract.push((*contract, changes));
            }
        }

        let executor = self.executor.clone();
        let coinbase_recipient = self.coinbase_recipient;
        let gas_price = self.gas_price;
        let header_to_produce = self.header_to_produce;
        let mut memory = self.memory_pool.take_raw();

        let future = {
            let storage_with_da = storage_with_da.clone();
            async move {
                let changes_per_contract: FxHashMap<ContractId, Changes> =
                    changes_per_contract.into_iter().collect();
                let memory_tx = InMemoryTransactionWithContracts::new(
                    storage_with_da,
                    changes_per_contract,
                );
                let mut storage_tx = StructuredStorage::new(memory_tx);

                let (transactions, execution_data) = executor
                    .execute_l2_transactions(
                        Components {
                            header_to_produce,
                            transactions_source: OnceTransactionsSource::new(
                                batch.transactions,
                            ),
                            coinbase_recipient,
                            gas_price,
                        },
                        &mut storage_tx,
                        start_idx_txs,
                        memory.as_mut(),
                    )
                    .await?;
                let coins_created = get_coins_outputs(
                    transactions.iter().zip(
                        execution_data
                            .tx_status
                            .iter()
                            .map(|tx_status| tx_status.id),
                    ),
                );
                if !execution_data.skipped_transactions.is_empty() {
                    for (tx_id, error) in execution_data.skipped_transactions.iter() {
                        batch.coins_used.retain(|coin| {
                            if coin.tx_id() == tx_id {
                                tracing::warn!("Transaction {tx_id} skipped: {error}");
                                false
                            } else {
                                true
                            }
                        });
                    }
                }

                let (changes, changes_per_contract) =
                    storage_tx.into_storage().into_changes();

                Ok(WorkSessionExecutionResult {
                    worker_id,
                    batch_id,
                    changes,
                    changes_per_contract,
                    coins_created,
                    coins_used: batch.coins_used,
                    message_nonces_used: batch.message_nonces_used,
                    contracts_used: batch.contracts_used,
                    skipped_tx: execution_data.skipped_transactions,
                    txs: transactions,
                    message_ids: execution_data.message_ids,
                    events: execution_data.events,
                    tx_statuses: execution_data.tx_status,
                    used_gas: execution_data.used_gas,
                    gas_diff: batch.gas.saturating_sub(execution_data.used_gas),
                    used_size: execution_data.used_size,
                    coinbase: execution_data.coinbase,
                })
            }
        };

        self.current_execution_tasks
            .push(self.runtime.spawn(future));
        Ok(())
    }

    fn register_execution_result(&mut self, res: WorkSessionExecutionResult) {
        for contract in res.contracts_used.iter() {
            self.current_executing_contracts.remove(contract);
        }

        for (contract_id, changes) in res.changes_per_contract {
            debug_assert!(!self.contracts_changes.contains_key(&contract_id));
            self.contracts_changes.insert(contract_id, changes);
        }

        self.state = SchedulerState::TransactionsReadyForPickup;

        self.gas_left = self.gas_left.saturating_add(res.gas_diff);

        self.execution_results.insert(
            res.batch_id,
            WorkSessionSavedData {
                changes: res.changes,
                message_nonces_used: res.message_nonces_used,
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

    async fn wait_all_execution_tasks(&mut self) -> Result<(), SchedulerError> {
        // We have reached the deadline
        // We need to merge the states of all the workers
        while !self.current_execution_tasks.is_empty() {
            match self.current_execution_tasks.next().await {
                Some(Ok(res)) => {
                    let res = res?;
                    if !res.skipped_tx.is_empty() {
                        drop(res.worker_id);
                        self.sequential_fallback(
                            res.batch_id,
                            res.txs,
                            res.coins_used,
                            res.coins_created,
                            res.message_nonces_used,
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
                                message_nonces_used: res.message_nonces_used,
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

        let now = Instant::now();
        if now > self.deadline {
            tracing::warn!(
                "Execution time exceeded the limit by: {}ms",
                now.checked_duration_since(self.deadline)
                    .expect("Checked above; qed")
                    .as_millis()
            );
        }
        Ok(())
    }

    fn verify_coherency_and_merge_results(
        &mut self,
        nb_batch: usize,
        l1_execution_data: L1ExecutionData,
        block_transaction: Arc<StorageTransaction<View>>,
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
            header: self.header_to_produce,
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
        let mut compiled_created_coins = CoinDependencyChainVerifier::new();
        let mut nonce_used = HashSet::new();
        for batch_id in 0..nb_batch {
            if let Some(changes) = self.execution_results.remove(&batch_id) {
                compiled_created_coins
                    .register_coins_created(batch_id, changes.coins_created);
                compiled_created_coins.verify_coins_used(
                    batch_id,
                    changes.coins_used.iter(),
                    &block_transaction,
                )?;
                for nonce in changes.message_nonces_used.iter() {
                    if !nonce_used.insert(*nonce) {
                        return Err(SchedulerError::InternalError(format!(
                            "Nonce {nonce} used multiple times."
                        )));
                    }
                }
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
        let contract_changes = core::mem::take(&mut self.contracts_changes);
        storage_changes.extend(contract_changes.into_values());
        exec_result.changes = StorageChanges::ChangesList(storage_changes);
        Ok(exec_result)
    }

    async fn execute_blob_transactions<D>(
        &mut self,
        mut storage: StorageTransaction<D>,
        start_idx_txs: u32,
    ) -> Result<(ExecutionData, Vec<Transaction>), SchedulerError>
    where
        D: KeyValueInspect<Column = Column>,
    {
        // Get a memory instance for the blob transactions execution
        let executor = self.executor.clone();
        let mut memory_instance = self.memory_pool.take_raw();
        let (transactions, mut execution_data) = executor
            .execute_l2_transactions(
                Components {
                    header_to_produce: self.header_to_produce,
                    transactions_source: OnceTransactionsSource::new(std::mem::take(
                        &mut self.blob_transactions,
                    )),
                    coinbase_recipient: self.coinbase_recipient,
                    gas_price: self.gas_price,
                },
                &mut storage,
                start_idx_txs,
                memory_instance.as_mut(),
            )
            .await?;
        execution_data.changes = storage.into_changes();

        Ok((execution_data, transactions))
    }

    // Wait for all the workers to finish gather all theirs transactions
    // re-execute them in one worker without skipped one. We also need to
    // fetch all the possible executed and stored batch after the lowest batch_id we gonna
    // re-execute.
    // Tell the TransactionSource that this transaction is skipped
    // to avoid sending new transactions that depend on it (using preconfirmation squeeze out)
    //
    // Can be replaced by a mechanism that replace the skipped_tx by a dummy transaction to not shift everything
    // TODO: Rework this function to continue the execution from the batch that got conflict
    //  and use the state of the contracts before execution of that batch.
    async fn sequential_fallback(
        &mut self,
        batch_id: usize,
        txs: Vec<Transaction>,
        coins_used: Vec<CoinInBatch>,
        coins_created: Vec<CoinInBatch>,
        message_nonces_used: Vec<Nonce>,
    ) -> Result<(), SchedulerError> {
        let block_height = *self.header_to_produce.height();
        let current_execution_tasks = std::mem::take(&mut self.current_execution_tasks);
        let mut lower_batch_id = batch_id;
        let mut higher_batch_id = batch_id;
        let mut all_txs_by_batch_id = FxHashMap::default();
        all_txs_by_batch_id.insert(
            batch_id,
            (txs, coins_created, coins_used, message_nonces_used),
        );
        for future in current_execution_tasks {
            match future.await {
                Ok(res) => {
                    let res = res?;
                    all_txs_by_batch_id.insert(
                        res.batch_id,
                        (
                            res.txs,
                            res.coins_created,
                            res.coins_used,
                            res.message_nonces_used,
                        ),
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

        let mut all_txs: Vec<MaybeCheckedTransaction> = vec![];
        let mut all_coins_created: Vec<CoinInBatch> = vec![];
        let mut all_coins_used: Vec<CoinInBatch> = vec![];
        let mut all_nonces_used: Vec<Nonce> = vec![];
        for id in lower_batch_id..=higher_batch_id {
            if let Some((txs, coins_created, coins_used, message_nonces_used)) =
                all_txs_by_batch_id.remove(&id)
            {
                for tx in txs {
                    let checked_tx = tx
                        .into_checked_basic(
                            block_height,
                            &self.consensus_parameters.clone(),
                        )
                        .map_err(|e| {
                            SchedulerError::InternalError(format!(
                                "Failed to convert transaction to checked: {e:?}"
                            ))
                        })?
                        .into();
                    all_txs.push(MaybeCheckedTransaction::CheckedTransaction(
                        checked_tx,
                        self.header_to_produce.consensus_parameters_version,
                    ));
                }
                all_coins_created.extend(coins_created);
                all_coins_used.extend(coins_used);
                all_nonces_used.extend(message_nonces_used);
            } else if let Some(res) = self.execution_results.remove(&id) {
                for tx in res.txs {
                    let checked_tx = tx
                        .into_checked(block_height, &self.consensus_parameters.clone())
                        .map_err(|e| {
                            SchedulerError::InternalError(format!(
                                "Failed to convert transaction to checked: {e:?}"
                            ))
                        })?
                        .into();
                    all_txs.push(MaybeCheckedTransaction::CheckedTransaction(
                        checked_tx,
                        self.header_to_produce.consensus_parameters_version,
                    ));
                }
                all_coins_created.extend(res.coins_created);
                all_coins_used.extend(res.coins_used);
                all_nonces_used.extend(res.message_nonces_used);
            } else {
                tracing::error!("Batch {id} not found in the execution results");
            }
        }

        let executor = self.executor.clone();
        // Get a memory instance for the blob transactions execution
        let mut memory_instance = self.memory_pool.take_raw();
        let view = self.storage.latest_view()?;
        let mut storage_tx = view.read_transaction();
        let (transactions, mut execution_data) = executor
            .execute_l2_transactions(
                Components {
                    header_to_produce: self.header_to_produce,
                    transactions_source: OnceTransactionsSource::new(all_txs),
                    coinbase_recipient: self.coinbase_recipient,
                    gas_price: self.gas_price,
                },
                &mut storage_tx,
                0,
                memory_instance.as_mut(),
            )
            .await?;
        execution_data.changes = storage_tx.into_changes();

        // Save execution results for all batch id with empty data
        // to not break the batch chain
        for id in lower_batch_id..=higher_batch_id {
            self.execution_results
                .insert(id, WorkSessionSavedData::default());
        }
        // Save the execution results for the current batch
        self.execution_results.insert(
            batch_id,
            WorkSessionSavedData {
                changes: execution_data.changes,
                coins_created: all_coins_created,
                message_nonces_used: all_nonces_used,
                coins_used: all_coins_used,
                txs: transactions,
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

#[allow(clippy::type_complexity)]
fn prepare_transactions_batch(
    consensus_params: &ConsensusParameters,
    batch: Vec<MaybeCheckedTransaction>,
) -> Result<PreparedBatch, SchedulerError> {
    let mut prepared_batch = PreparedBatch::default();

    for (idx, tx) in batch.into_iter().enumerate() {
        let tx_id = tx.id(&consensus_params.chain_id());
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
                fuel_core_types::fuel_tx::Input::MessageCoinPredicate(message) => {
                    prepared_batch.message_nonces_used.push(message.nonce);
                }
                fuel_core_types::fuel_tx::Input::MessageCoinSigned(message) => {
                    prepared_batch.message_nonces_used.push(message.nonce);
                }
                fuel_core_types::fuel_tx::Input::MessageDataPredicate(message) => {
                    prepared_batch.message_nonces_used.push(message.nonce);
                }
                fuel_core_types::fuel_tx::Input::MessageDataSigned(message) => {
                    prepared_batch.message_nonces_used.push(message.nonce);
                }
            }
        }

        for output in tx.outputs().iter() {
            if let Output::ContractCreated { contract_id, .. } = output {
                prepared_batch.contracts_used.push(*contract_id);
            }
        }

        let is_blob = tx.is_blob();
        prepared_batch.total_size =
            prepared_batch.total_size.saturating_add(tx.size() as u64);
        prepared_batch.number_of_transactions =
            prepared_batch.number_of_transactions.saturating_add(1);
        let max_gas = tx.max_gas(consensus_params)?;
        if is_blob {
            prepared_batch.blob_gas = prepared_batch.blob_gas.saturating_add(max_gas);
            prepared_batch.blob_transactions.push(tx);
        } else {
            prepared_batch.gas = prepared_batch.gas.saturating_add(max_gas);
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
        for (output_idx, output) in tx.outputs().iter().enumerate() {
            match output {
                Output::Coin {
                    to,
                    amount,
                    asset_id,
                } => {
                    coins.push(CoinInBatch::from_output(
                        UtxoId::new(
                            tx_id,
                            u16::try_from(output_idx)
                                .expect("Output index should fit in u16"),
                        ),
                        idx,
                        tx_id,
                        *to,
                        *amount,
                        *asset_id,
                    ));
                }
                Output::Change {
                    to,
                    amount,
                    asset_id,
                } => {
                    coins.push(CoinInBatch::from_output(
                        UtxoId::new(
                            tx_id,
                            u16::try_from(output_idx)
                                .expect("Output index should fit in u16"),
                        ),
                        idx,
                        tx_id,
                        *to,
                        *amount,
                        *asset_id,
                    ));
                }
                Output::Variable {
                    to,
                    amount,
                    asset_id,
                } => {
                    coins.push(CoinInBatch::from_output(
                        UtxoId::new(
                            tx_id,
                            u16::try_from(output_idx)
                                .expect("Output index should fit in u16"),
                        ),
                        idx,
                        tx_id,
                        *to,
                        *amount,
                        *asset_id,
                    ));
                }
                _ => {}
            }
        }
    }
    coins
}
