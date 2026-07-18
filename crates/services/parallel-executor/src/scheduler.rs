use crate::{
    config::{
        Config,
        WorkerCountPolicy,
    },
    in_memory_transaction_with_contracts::InMemoryTransactionWithContracts,
    l1_execution_data::L1ExecutionData,
    memory::MemoryPool,
    once_transaction_source::OnceTransactionsSource,
    ports::{
        BatchExecutionReport,
        BatchFeedbackHandle,
        Filter,
        TransactionsSource,
    },
    scheduler::workers::{
        WorkerId,
        WorkerPool,
    },
    tx_waiter::NoWaitTxs,
};
use ::fuel_core_metrics as parallel_executor_metrics;
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
    kv_store::{
        KeyValueInspect,
        WriteOperation,
    },
    structured_storage::StructuredStorage,
    transactional::{
        AtomicView,
        Changes,
        ConflictPolicy,
        IntoTransaction,
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
        UniqueIdentifier,
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
use std::{
    collections::{
        HashMap,
        HashSet,
        btree_map,
    },
    sync::{
        Arc,
        atomic::{
            AtomicU32,
            Ordering,
        },
    },
    time::Duration,
};
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
    /// Batch preparation stats keyed by batch id
    batch_preparations: Option<HashMap<usize, BatchPreparationStats>>,
    /// Producer feedback handles keyed by (internal) batch id, awaiting the
    /// batch's completion so the measured timings can be reported back. Only
    /// populated for batches whose producer requested feedback (i.e. the txpool
    /// lane scheduler is enabled); empty otherwise, so the flag-off path is
    /// untouched.
    batch_feedback: FxHashMap<usize, PendingBatchFeedback>,
    /// Per-batch snapshot of the accumulated per-contract `Changes` that each
    /// dispatched batch was SEEDED with (i.e. the state of those contracts as
    /// left by all previously-dispatched batches). Populated in
    /// [`Self::execute_batch`] with a clone of the seed handed to the worker,
    /// but ONLY for contracts that already carried accumulated changes — a batch
    /// touching only fresh contracts stores nothing, so the common path pays
    /// only for genuinely re-touched (contended) contracts.
    ///
    /// This is the reconstruction source for [`Self::sequential_fallback`]: when
    /// a runtime hazard forces a batch-id range to be re-executed sequentially,
    /// the replay must observe the contract state left by the KEPT earlier
    /// batches (id `< lower`), which was moved out of `contracts_changes` when
    /// the in-range batches picked those contracts up (and then discarded with
    /// the aborted parallel results). The seed handed to the lowest-id in-range
    /// batch that touched a contract is exactly that as-of-`lower` state, so the
    /// fallback rebuilds its replay seed from here rather than from the (stale)
    /// pre-block view. Entries are consumed by the fallback and otherwise freed
    /// when the per-block scheduler is dropped.
    dispatched_contract_seeds: FxHashMap<usize, FxHashMap<ContractId, Changes>>,
    /// The block-relative starting transaction index each dispatched batch was
    /// given (`start_idx_txs`, i.e. the count of block txs scheduled before it).
    /// `sequential_fallback` replays the `[lower, higher]` range starting at the
    /// index the lowest-id batch in the range originally used, so the replayed
    /// txs land at the same block positions the sequential executor assigns —
    /// their `TxPointer` (hence e.g. `ContractsLatestUtxo`) must match, or the
    /// producer diverges from the validator. Populated for every dispatched
    /// batch; freed with the per-block scheduler.
    dispatched_start_idx: FxHashMap<usize, u32>,
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
    /// Counters for tracking worker concurrency when metrics are enabled
    worker_counters: Option<WorkerCounters>,
    /// Per-block time-spend accumulators (only meaningful when metrics are on;
    /// the adds are cheap and happen at the existing timing sites). Emitted once
    /// as the block-summary decomposition at the end of [`Self::run`].
    time_accounting: TimeAccounting,
}

/// Cheap per-block accumulators feeding the time-spend block summary. Each field
/// is summed at the site where that phase's duration is already measured, so no
/// new timers run in hot loops (only one extra timer wraps the per-ask txpool
/// call, at the same cadence as the existing batch-prepare timer).
#[derive(Default)]
struct TimeAccounting {
    /// Elapsed-from-block-start at the first batch dispatch.
    first_dispatch: Option<Duration>,
    /// `prepare_transactions_batch` cost (batch prepare minus the txpool ask).
    prepare: Duration,
    /// Time blocked in `get_executable_transactions` (the txpool ask).
    pool_ask: Duration,
    /// Per-contract `Changes` handoff, both split and merge sides.
    handoff: Duration,
    /// Sequential-fallback re-execution.
    fallback: Duration,
    /// Worker-seconds actually spent executing batch inner work (sum over
    /// completed and discarded batches).
    worker_busy: Duration,
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
    /// Execution time for this batch
    execution_duration: Duration,
}

#[derive(Clone, Copy)]
struct BatchPreparationStats {
    duration: Duration,
    tx_count: u32,
    gas: u64,
}

/// A producer feedback handle held until its batch completes, together with the
/// parallelization overhead accumulated for the batch so far. On completion the
/// inner execution time is added as a separate field and the report is sent.
struct PendingBatchFeedback {
    handle: BatchFeedbackHandle,
    /// Parallelization overhead directly attributable to this batch. Accrued at
    /// three sites: batch preparation (at dispatch), the split side of the
    /// per-contract `Changes` handoff (taking changes into the worker, in
    /// `execute_batch`), and the merge side (re-inserting them on completion, in
    /// `register_execution_result`).
    ///
    /// Attribution choice: only costs that are *directly per-batch measurable*
    /// are folded in here. The block-level coherency+merge stage
    /// (`verify_coherency_and_merge_results` + the canonical fold) is a single
    /// whole-block cost with no non-arbitrary per-batch split, so it is
    /// deliberately EXCLUDED from `overhead_time` and instead exported on its own
    /// (`record_block_merge`). This keeps the per-batch `overhead_time` the lane
    /// scheduler's EMA consumes an unbiased sum of that batch's own overhead,
    /// while the block-level merge cost remains observable as an aggregate.
    overhead: Duration,
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
        // `blob_execution_data.changes` is the fully-merged block state at the
        // point blobs run: the base view plus DA + every batch/contract change
        // (seeded into the blob transaction) plus the blob txs' own writes. So
        // this field is expected to be *non-empty* — it carries all the changes
        // from all executions, as the comment above says. The previous
        // `debug_assert!(self.changes.is_empty(), ..)` was self-contradictory
        // (it fired immediately after assigning a non-empty value) and panicked
        // every debug-build block that contained a blob.
        self.changes = StorageChanges::Changes(blob_execution_data.changes);
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

/// A source batch prepared for dispatch: the checked/derived [`PreparedBatch`]
/// plus the metadata that travels with it to the dispatch site.
struct ReadyBatch {
    batch: PreparedBatch,
    anchor_contract_ids: Vec<ContractId>,
    feedback_handle: Option<BatchFeedbackHandle>,
    /// This batch's share of the round's ask+prepare cost (the shared cost is
    /// split evenly across the round's batches).
    prepare_duration: Duration,
}

/// The outcome of one ask round: the prepared batches (guaranteed non-empty)
/// and whether the source's answer covered all requested workers (see
/// [`crate::ports::TransactionSourceExecutableTransactions::answered_all_workers`]).
struct ReadyBatches {
    batches: Vec<ReadyBatch>,
    answered_all_workers: bool,
}

#[derive(Clone)]
struct WorkerCounters {
    current: Arc<AtomicU32>,
    max: Arc<AtomicU32>,
}

impl WorkerCounters {
    fn new() -> Self {
        Self {
            current: Arc::new(AtomicU32::new(0)),
            max: Arc::new(AtomicU32::new(0)),
        }
    }

    fn record_started(&self) {
        let current = self.current.fetch_add(1, Ordering::Relaxed) + 1;
        self.max.fetch_max(current, Ordering::Relaxed);
    }

    fn reset(&self) {
        self.current.store(0, Ordering::Relaxed);
        self.max.store(0, Ordering::Relaxed);
    }
}

struct WorkerCountGuard {
    current: Arc<AtomicU32>,
}

impl Drop for WorkerCountGuard {
    fn drop(&mut self) {
        self.current.fetch_sub(1, Ordering::Relaxed);
    }
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
        let batch_preparations = config.metrics.then(HashMap::new);
        let worker_counters = config.metrics.then(WorkerCounters::new);
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
            worker_pool: WorkerPool::new(config.worker_count.get()),
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
            batch_preparations,
            batch_feedback: FxHashMap::default(),
            dispatched_contract_seeds: FxHashMap::default(),
            dispatched_start_idx: FxHashMap::default(),
            worker_counters,
            time_accounting: TimeAccounting::default(),
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
        let instant = Instant::now();
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
        let mut execution_time_recorded = false;
        let mut nb_batch_created = 0;
        let mut nb_transactions: u32 = l1_execution_data.tx_count;
        let mut non_empty_batch_tx_counts = self.config.metrics.then(Vec::new);
        let mut non_empty_batch_allocated_gas = self.config.metrics.then(Vec::new);
        let mut non_empty_batch_used_gas = self.config.metrics.then(Vec::new);
        let mut non_empty_batch_anchors = self.config.metrics.then(Vec::new);
        let batch_metrics_block_height = self
            .config
            .metrics
            .then(parallel_executor_metrics::next_debug_batch_metrics_block_height)
            .unwrap_or(0);
        let initial_gas_per_worker = self
            .consensus_parameters
            .block_gas_limit()
            .checked_div(self.config.worker_count.get() as u64)
            .ok_or(SchedulerError::InternalError(
                "Invalid block gas limit".to_string(),
            ))?
            .checked_sub(l1_execution_data.used_gas)
            .ok_or(SchedulerError::InternalError(
                "L1 transactions consumed all the gas".to_string(),
            ))?;
        let mut total_gas: u64 = 0;

        tracing::warn!("scheduler starting run loop at {:?}", instant.elapsed());
        'outer: loop {
            let tx_notifier = if new_tx_notifier.has_changed().is_ok() {
                Either::Left(new_tx_notifier.changed())
            } else {
                // If the notifier is closed, we never get new transactions
                Either::Right(futures::future::pending())
            };

            if self.is_worker_idling() {
                // ONE ask per dispatch round: request batches for ALL currently
                // free workers in a single call, then dispatch every returned
                // batch to its own worker exactly as received (the lane
                // scheduler's proposals are conflict-free by construction and
                // must not be re-packed). The classic (non-lane) txpool path
                // still answers with a single batch per ask, keeping its
                // historical ask cadence.
                let selection_worker_count = self.selection_worker_count();
                let free_worker_count = self.worker_pool.available_workers();
                let ready_batches = self
                    .ask_new_transaction_batches(
                        tx_source,
                        now,
                        initial_gas_per_worker,
                        selection_worker_count,
                        free_worker_count,
                    )
                    .await?;

                let Some(ready_batches) = ready_batches else {
                    self.state = SchedulerState::NoTransactionsForPickup;
                    tracing::warn!(
                        "No transactions to execute, waiting for new transactions or workers to finish"
                    );
                    continue 'outer;
                };
                let returned_batches = ready_batches.batches.len();

                for ready in ready_batches.batches {
                    // The source must never return more batches than the free
                    // workers the ask covered: extracted transactions cannot be
                    // handed back, so failing loudly beats dropping them.
                    if self.worker_pool.is_empty() {
                        return Err(SchedulerError::InternalError(
                            "Transaction source returned more batches than free workers"
                                .to_string(),
                        ));
                    }
                    let ReadyBatch {
                        batch,
                        anchor_contract_ids,
                        feedback_handle,
                        prepare_duration,
                    } = ready;
                    tracing::warn!(
                        "new batch id {:?} prepared at: {:?}",
                        nb_batch_created,
                        instant.elapsed()
                    );
                    let batch_len = batch.number_of_transactions;
                    if self.config.metrics {
                        if let Some(batch_tx_counts) = non_empty_batch_tx_counts.as_mut()
                        {
                            batch_tx_counts.push(batch_len);
                        }
                        if let Some(batch_allocated_gas) =
                            non_empty_batch_allocated_gas.as_mut()
                        {
                            batch_allocated_gas.push(batch.gas);
                        }
                        if let Some(batch_used_gas) = non_empty_batch_used_gas.as_mut() {
                            batch_used_gas.push(0);
                        }
                        if let Some(batch_anchors) = non_empty_batch_anchors.as_mut() {
                            batch_anchors.push(anchor_contract_ids);
                        }
                        parallel_executor_metrics::record_batch_prepare(
                            prepare_duration,
                            batch_len,
                            batch.gas,
                        );
                        if self.time_accounting.first_dispatch.is_none() {
                            self.time_accounting.first_dispatch = Some(instant.elapsed());
                        }
                        if let Some(batch_preparations) = self.batch_preparations.as_mut()
                        {
                            batch_preparations.insert(
                                nb_batch_created,
                                BatchPreparationStats {
                                    duration: prepare_duration,
                                    tx_count: batch_len,
                                    gas: batch.gas,
                                },
                            );
                        }
                        total_gas = total_gas.saturating_add(batch.gas);
                    }

                    // Hold the producer feedback handle (if any) until this
                    // batch's execution result is registered, so its measured
                    // timings can be reported back. `feedback_handle` is `None`
                    // (and this map stays empty) unless the txpool lane
                    // scheduler is enabled. A batch that never reaches
                    // completion simply drops its handle (silent-safe by
                    // design).
                    if let Some(handle) = feedback_handle {
                        self.batch_feedback.insert(
                            nb_batch_created,
                            PendingBatchFeedback {
                                handle,
                                overhead: prepare_duration,
                            },
                        );
                    }

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
                }

                // The lane-scheduler path answers the WHOLE ask at once: fewer
                // batches than requested workers means nothing more is
                // schedulable right now, so wait for a state change (a batch
                // completion or a new-transaction notification) instead of
                // immediately re-asking for the leftover workers. The classic
                // path (`answered_all_workers == false`) keeps its historical
                // behavior of asking again for the remaining workers.
                if ready_batches.answered_all_workers
                    && returned_batches < free_worker_count
                {
                    self.state = SchedulerState::NoTransactionsForPickup;
                }
            } else if self.current_execution_tasks.is_empty() {
                let waiting = Instant::now();
                tokio::select! {
                    _ = tx_notifier => {
                        self.state = SchedulerState::TransactionsReadyForPickup;
                    }
                    _ = tokio::time::sleep_until(deadline) => {
                        if !execution_time_recorded {
                            let execution_time = instant
                                .elapsed()
                                .saturating_sub(waiting.elapsed());
                            parallel_executor_metrics::record_execution_time(execution_time);
                            execution_time_recorded = true;
                        }
                        tracing::warn!("******");
                        tracing::warn!("waited until deadline for {:?}, total elapsed: {:?}", waiting.elapsed(), instant.elapsed());
                        break 'outer;
                    }
                }
            } else {
                tracing::warn!("Waiting for workers to finish");
                tokio::select! {
                    _ = tx_notifier => {
                        tracing::warn!("New transactions received");
                        self.state = SchedulerState::TransactionsReadyForPickup;
                    }
                    result = self.current_execution_tasks.select_next_some() => {
                        tracing::warn!("Worker finished at {:?}", instant.elapsed());
                        match result {
                            Ok(res) => {
                                let res = res?;
                                if !res.skipped_tx.is_empty() {
                                    // Fallback consumes this batch (and every
                                    // other in-flight one); it reports their
                                    // feedback as `completed: false` and clears
                                    // their `batch_preparations`/`batch_feedback`
                                    // bookkeeping — so no explicit cleanup here.
                                    drop(res.worker_id);
                                    // The fallback drops the range's skipped txs,
                                    // so the running block-tx counter must be
                                    // reset to the true committed count (the range
                                    // start + what the replay committed) — else
                                    // every later batch/blob gets a shifted
                                    // `start_idx_txs` and its `TxPointer`s diverge
                                    // from the sequential validator.
                                    nb_transactions = self.sequential_fallback(res.batch_id, res.txs, res.coins_used, res.coins_created, res.message_nonces_used, res.execution_duration, storage_with_da.clone()).await?;
                                    continue;
                                }

                                if self.config.metrics {
                                    if let Some(batch_used_gas) =
                                        non_empty_batch_used_gas.as_mut()
                                    {
                                        if let Some(slot) = batch_used_gas
                                            .get_mut(res.batch_id)
                                        {
                                            *slot = res.used_gas;
                                        }
                                    }
                                    if let Some(batch_preparations) =
                                        self.batch_preparations.as_mut()
                                    {
                                        if let Some(prep) =
                                            batch_preparations.remove(&res.batch_id)
                                        {
                                            let gas_for_norm =
                                                if res.used_gas > 0 {
                                                    res.used_gas
                                                } else {
                                                    prep.gas
                                                };
                                            parallel_executor_metrics::record_batch_execute(
                                                res.execution_duration,
                                                prep.tx_count,
                                                gas_for_norm,
                                            );
                                            parallel_executor_metrics::record_batch_total(
                                                prep.duration.saturating_add(
                                                    res.execution_duration,
                                                ),
                                                prep.tx_count,
                                                gas_for_norm,
                                            );
                                        }
                                    }
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
                        tracing::warn!("timeout waiting on workers");
                        break 'outer;
                    }
                }
            }
        }

        tracing::warn!("******");
        tracing::warn!("waiting for execution tasks: {:?}", instant.elapsed());
        let (exceeded_deadline, fallback_next_start_idx) = self
            .wait_all_execution_tasks(storage_with_da.clone())
            .await?;
        if let Some(next_idx) = fallback_next_start_idx {
            // A drain-time fallback dropped skipped txs; keep the block-tx
            // counter contiguous for the blob stage / metrics below.
            nb_transactions = next_idx;
        }
        tracing::warn!("execution tasks done: {:?}", instant.elapsed());
        if self.config.metrics {
            if exceeded_deadline && !execution_time_recorded {
                parallel_executor_metrics::record_execution_time(instant.elapsed());
            }
            parallel_executor_metrics::set_number_of_transactions(nb_transactions);
            parallel_executor_metrics::set_total_gas_used(total_gas);
            let block_height = u32::from(*self.header_to_produce.height());
            parallel_executor_metrics::set_block_height(block_height);
            parallel_executor_metrics::set_max_workers_used(
                self.worker_counters
                    .as_ref()
                    .map(|counters| counters.max.load(Ordering::Relaxed))
                    .unwrap_or(0),
            );
            parallel_executor_metrics::set_non_empty_batch_transactions(
                batch_metrics_block_height,
                non_empty_batch_tx_counts.as_deref().unwrap_or(&[]),
            );
            parallel_executor_metrics::set_non_empty_batch_allocated_gas(
                batch_metrics_block_height,
                non_empty_batch_allocated_gas.as_deref().unwrap_or(&[]),
            );
            parallel_executor_metrics::set_non_empty_batch_used_gas(
                batch_metrics_block_height,
                non_empty_batch_used_gas.as_deref().unwrap_or(&[]),
            );
            parallel_executor_metrics::set_batch_anchor_contracts(
                batch_metrics_block_height,
                non_empty_batch_anchors.as_deref().unwrap_or(&[]),
            );
            if let Some(counters) = self.worker_counters.as_ref() {
                counters.reset();
            }
        }

        // let mut res = self.verify_coherency_and_merge_results(
        //     nb_batch_created,
        //     l1_execution_data,
        //     storage_with_da.clone(),
        // )?;

        // Time the block-level coherency + merge stage: coin/nonce verification
        // and the canonical fold(s). Blob execution (below) is a separate cost
        // and is excluded from this measurement.
        let merge_start = Instant::now();

        let result = self.verify_coherency_and_merge_results(
            nb_batch_created,
            l1_execution_data,
            storage_with_da.clone(),
        );

        if result.is_err() {
            tracing::warn!("coherency result: {:?}", result);
        }

        let mut res = result?;

        tracing::warn!("scheduler done: {:?}", instant.elapsed());

        // FIX 1 — coalesce same-key operations across the parallel batches'
        // (and per-contract) `Changes` into a single, already-merged `Changes`.
        //
        // `verify_coherency_and_merge_results` returns a `ChangesList` with one
        // entry per batch (in batch-id order) followed by the per-contract
        // entries. A coin created in batch `i` and spent in batch `j > i` lands
        // as an `Insert` in entry `i` and a `Remove` in entry `j` — legal (the
        // block orders creation before spend), but the strict conflict finder in
        // `RocksDb::commit_changes` (and the blob path's `Fail`-policy commit)
        // rejects the same key appearing in two entries. Folding the list in
        // canonical order collapses that pair to a net `Remove`, exactly what
        // the SEQUENTIAL executor's single coalescing map produces for the same
        // block. Emitting one merged `Changes` — the same shape the sequential
        // executor/validator emits — is what keeps producer and validator
        // symmetric: every node commits one coalesced map, so the same block is
        // accepted everywhere.
        let mut merged = fold_changes_in_canonical_order(
            core::mem::take(&mut res.changes).extract_list_of_changes(),
        )?;
        // Merge time accrued so far (verify + first fold), before the blob stage.
        let mut merge_elapsed = merge_start.elapsed();

        if !self.blob_transactions.is_empty() {
            // Execute the blob txs against the fully-merged block state so far:
            // base view + DA changes (both carried by `storage_with_da`) plus
            // every batch/contract change (`merged`). `Overwrite` here is safe —
            // `merged` is already conflict-checked by the fold above, and blob
            // txs run sequentially on top (a blob spending a batch-created coin
            // simply turns that `Insert` into a `Remove`, matching sequential).
            let tx = StorageTransaction::transaction(
                storage_with_da.clone(),
                ConflictPolicy::Overwrite,
                merged,
            );

            let (blob_execution_data, blob_txs) =
                self.execute_blob_transactions(tx, nb_transactions).await?;
            tracing::warn!("blob execution done: {:?}", instant.elapsed());
            res.add_blob_execution_data(blob_execution_data, blob_txs);
            tracing::warn!("blob execution data added: {:?}", instant.elapsed());
            merged = match core::mem::take(&mut res.changes) {
                StorageChanges::Changes(changes) => changes,
                StorageChanges::ChangesList(list) => {
                    fold_changes_in_canonical_order(list)?
                }
            };
        }

        // Fold the DA-import changes in FIRST (their canonical position is
        // before batch 0): a DA-imported message is `Insert`-ed in `da_changes`
        // and `Remove`-d by its single in-block consumer inside `merged`, so
        // `[Insert(da), Remove(batch)]` folds to a net `Remove` — the message is
        // consumed. `da_changes` never inserts a key that a batch also inserts,
        // so this fold only ever pairs DA inserts with batch removes.
        // TODO: Avoid cloning the DA changes
        let da_fold_start = Instant::now();
        let da_changes = storage_with_da.changes().clone();
        let final_changes = fold_changes_in_canonical_order(vec![da_changes, merged])?;
        res.changes = StorageChanges::Changes(final_changes);
        merge_elapsed = merge_elapsed.saturating_add(da_fold_start.elapsed());

        let execution_time = instant.elapsed();
        tracing::warn!("Scheduler `run` execution time: {:?}", execution_time);
        if self.config.metrics {
            parallel_executor_metrics::record_block_merge(
                merge_elapsed,
                nb_transactions,
                total_gas,
            );
            parallel_executor_metrics::record_scheduler_run_time(execution_time);

            // Block-summary time-spend decomposition: where and how much time the
            // scheduler spent this block. `prepare` excludes the txpool ask
            // (reported separately as `pool_ask`); `worker_busy` is the parallel
            // worker-seconds spent executing (occupancy vs `worker_available`),
            // shown alongside the serial phases rather than summed into them.
            let worker_count = self.config.worker_count.get() as u32;
            let worker_available = execution_time
                .checked_mul(worker_count)
                .unwrap_or(execution_time);
            let prepare = self
                .time_accounting
                .prepare
                .saturating_sub(self.time_accounting.pool_ask);
            let decomposition = parallel_executor_metrics::BlockTimeDecomposition {
                window: execution_time,
                prepare,
                pool_ask: self.time_accounting.pool_ask,
                handoff: self.time_accounting.handoff,
                merge: merge_elapsed,
                fallback: self.time_accounting.fallback,
                first_dispatch: self.time_accounting.first_dispatch.unwrap_or_default(),
                worker_busy: self.time_accounting.worker_busy,
                worker_available,
            };
            let util = if worker_available.as_secs_f64() > 0.0 {
                100.0 * self.time_accounting.worker_busy.as_secs_f64()
                    / worker_available.as_secs_f64()
            } else {
                0.0
            };
            tracing::info!(
                target: "parallel_executor::block_summary",
                height = u32::from(*self.header_to_produce.height()),
                txs = nb_transactions,
                window_ms = execution_time.as_millis() as u64,
                first_dispatch_ms = decomposition.first_dispatch.as_millis() as u64,
                prepare_ms = prepare.as_millis() as u64,
                pool_ask_ms = self.time_accounting.pool_ask.as_millis() as u64,
                execute_worker_ms = self.time_accounting.worker_busy.as_millis() as u64,
                handoff_ms = self.time_accounting.handoff.as_millis() as u64,
                merge_ms = merge_elapsed.as_millis() as u64,
                fallback_ms = self.time_accounting.fallback.as_millis() as u64,
                idle_ms = decomposition.idle().as_millis() as u64,
                worker_util_pct = util,
                "block time-spend decomposition (prepare/pool_ask/execute/handoff/merge/fallback/idle)",
            );
            parallel_executor_metrics::record_block_time_decomposition(decomposition);
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

    /// One ask covering `free_worker_count` workers: fetch the source's next
    /// executable batches, prepare each one (tx checking / metadata derivation
    /// — NO re-packing: batches execute exactly as received), account the
    /// blob transactions, and charge the block budgets.
    ///
    /// Returns `None` when the source yielded no executable (non-blob)
    /// transactions at all, otherwise the prepared batches ready for dispatch.
    async fn ask_new_transaction_batches<TxSource>(
        &mut self,
        tx_source: &TxSource,
        start_execution_time: Instant,
        initial_gas_per_core: u64,
        selection_worker_count: usize,
        free_worker_count: usize,
    ) -> Result<Option<ReadyBatches>, SchedulerError>
    where
        TxSource: TransactionsSource,
    {
        let instant = Instant::now();
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
        // The block's total remaining declared-gas budget across all workers:
        // the source must keep the CUMULATIVE gas of the returned batches under
        // it (per-worker `current_gas` budgets alone may sum above it).
        let total_gas_limit = self.gas_left.saturating_sub(self.blob_gas);

        // Time the txpool ask (how long the scheduler blocks waiting for the pool
        // to hand back the batches) — one of the "where does time go" signals.
        let pool_ask_start = Instant::now();
        let executable_transactions = tx_source
            .get_executable_transactions(
                current_gas,
                total_gas_limit,
                self.tx_left,
                self.tx_size_left,
                selection_worker_count,
                free_worker_count,
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
        if self.config.metrics {
            self.time_accounting.pool_ask = self
                .time_accounting
                .pool_ask
                .saturating_add(pool_ask_start.elapsed());
            let returned_txs = executable_transactions
                .batches
                .iter()
                .map(|batch| batch.transactions.len())
                .sum::<usize>();
            parallel_executor_metrics::record_pool_ask(
                executable_transactions.batches.len(),
                returned_txs,
            );
        }
        self.current_executing_contracts =
            executable_transactions.filter.excluded_contract_ids;
        let answered_all_workers = executable_transactions.answered_all_workers;

        let mut batches = Vec::with_capacity(executable_transactions.batches.len());
        for source_batch in executable_transactions.batches {
            let prepared_batch = prepare_transactions_batch(
                &self.consensus_parameters,
                source_batch.transactions,
            )?;
            self.update_constraints(
                prepared_batch.number_of_transactions,
                prepared_batch.total_size,
                prepared_batch.gas,
            )?;
            let mut prepared_batch = prepared_batch;
            let blob_transactions =
                core::mem::take(&mut prepared_batch.blob_transactions);
            self.blob_transactions.extend(blob_transactions.into_iter());
            self.blob_gas = self.blob_gas.saturating_add(prepared_batch.blob_gas);
            if prepared_batch.transactions.is_empty() {
                // Blob-only (or empty) batch: nothing to dispatch onto a worker.
                // Its feedback handle (if any) is dropped, which the producer
                // tolerates by design.
                continue;
            }
            batches.push(ReadyBatch {
                batch: prepared_batch,
                anchor_contract_ids: source_batch.anchor_contract_ids,
                feedback_handle: source_batch.feedback_handle,
                // Placeholder; the shared preparation cost is split evenly over
                // the returned batches right below.
                prepare_duration: Duration::default(),
            });
        }

        // Split the shared ask+prepare cost of this round evenly across its
        // batches: it is the per-batch parallelization overhead the producer's
        // feedback loop consumes, and per-ask fixed protocol cost is exactly
        // what the multi-batch protocol amortizes.
        let prepare_total = instant.elapsed();
        let batch_count = batches.len();
        if batch_count > 0 {
            let share = prepare_total
                .checked_div(batch_count as u32)
                .unwrap_or_default();
            for ready in batches.iter_mut() {
                ready.prepare_duration = share;
            }
            if self.config.metrics {
                // `prepare_total` includes the txpool ask; the `prepare` phase
                // subtracts `pool_ask` at emit time so the two do not
                // double-count in the block summary.
                self.time_accounting.prepare =
                    self.time_accounting.prepare.saturating_add(prepare_total);
            }
        }
        tracing::warn!(
            "new batches prepared in: {:?}, {:?} batches, for {:?} txs",
            instant.elapsed(),
            batch_count,
            batches
                .iter()
                .map(|b| b.batch.number_of_transactions)
                .sum::<u32>(),
        );
        if batch_count == 0 {
            Ok(None)
        } else {
            Ok(Some(ReadyBatches {
                batches,
                answered_all_workers,
            }))
        }
    }

    fn selection_worker_count(&self) -> usize {
        match self.config.worker_count_policy {
            WorkerCountPolicy::StaticMax => self.config.worker_count.get(),
            WorkerCountPolicy::DynamicIdle => self.worker_pool.available_workers(),
        }
    }

    fn execute_batch(
        &mut self,
        mut batch: PreparedBatch,
        batch_id: usize,
        start_idx_txs: u32,
        storage_with_da: Arc<StorageTransaction<View>>,
    ) -> Result<(), SchedulerError> {
        let input_tx_ids = batch
            .transactions
            .iter()
            .map(|tx| tx.id(&self.consensus_parameters.chain_id()))
            .collect::<Vec<_>>();
        let chain_id = self.consensus_parameters.chain_id();
        let worker_id =
            self.worker_pool
                .take_worker()
                .ok_or(SchedulerError::InternalError(
                    "No available workers".to_string(),
                ))?;
        let worker_counters = self.worker_counters.clone();

        let mut changes_per_contract = Vec::with_capacity(batch.contracts_used.len());

        // Split side of the per-contract `Changes` handoff: take each contract's
        // accumulated changes out of the shared map to hand into the worker.
        let split_start = Instant::now();
        for contract in batch.contracts_used.iter() {
            self.current_executing_contracts.insert(*contract);
            if let Some(changes) = self.contracts_changes.remove(contract) {
                changes_per_contract.push((*contract, changes));
            }
        }
        let split_duration = split_start.elapsed();
        let handoff_contract_count = changes_per_contract.len();
        let handoff_changeset_keys: usize = changes_per_contract
            .iter()
            .map(|(_, changes)| {
                changes.values().map(|column| column.len()).sum::<usize>()
            })
            .sum();
        if self.config.metrics {
            parallel_executor_metrics::record_contract_handoff_split(
                split_duration,
                handoff_contract_count,
                handoff_changeset_keys,
            );
            self.time_accounting.handoff =
                self.time_accounting.handoff.saturating_add(split_duration);
        }
        // Attribute this batch's split-handoff cost to its overhead (the merge
        // side is added on completion). Only present when the producer requested
        // feedback (lane scheduler on).
        if let Some(pending) = self.batch_feedback.get_mut(&batch_id) {
            pending.overhead = pending.overhead.saturating_add(split_duration);
        }

        // Snapshot the seed this batch is dispatched with (the accumulated
        // per-contract state of everything scheduled before it) so
        // `sequential_fallback` can rebuild the correct as-of-`lower` replay
        // view if this or a neighbouring batch later aborts. Only non-empty
        // seeds (contracts that already carried changes) are stored, so a batch
        // over fresh contracts adds nothing.
        if !changes_per_contract.is_empty() {
            self.dispatched_contract_seeds
                .insert(batch_id, changes_per_contract.iter().cloned().collect());
        }
        self.dispatched_start_idx.insert(batch_id, start_idx_txs);

        let executor = self.executor.clone();
        let coinbase_recipient = self.coinbase_recipient;
        let gas_price = self.gas_price;
        let header_to_produce = self.header_to_produce;
        let mut memory = self.memory_pool.take_raw();

        let future = {
            let instant = Instant::now();
            let storage_with_da = storage_with_da.clone();
            async move {
                let _worker_guard = worker_counters.as_ref().map(|counters| {
                    counters.record_started();
                    WorkerCountGuard {
                        current: counters.current.clone(),
                    }
                });
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
                let returned_tx_ids = transactions
                    .iter()
                    .map(|tx| tx.id(&chain_id))
                    .collect::<Vec<_>>();
                let skipped_errors = execution_data
                    .skipped_transactions
                    .iter()
                    .map(|(tx_id, error)| format!("{tx_id}: {error}"))
                    .collect::<Vec<_>>()
                    .join("; ");
                if input_tx_ids.len() <= 4
                    || transactions.len() != input_tx_ids.len()
                    || execution_data.used_gas == 0
                {
                    eprintln!(
                        "parallel executor batch {batch_id}: input_count={} returned_count={} skipped_count={} used_gas={} input_ids=[{}] returned_ids=[{}] skipped_errors=[{}]",
                        input_tx_ids.len(),
                        transactions.len(),
                        execution_data.skipped_transactions.len(),
                        execution_data.used_gas,
                        format_tx_ids(input_tx_ids.iter().copied()),
                        format_tx_ids(returned_tx_ids.iter().copied()),
                        skipped_errors,
                    );
                }
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

                let batch_duration = instant.elapsed();
                tracing::warn!(
                    "batch {:?} duration: {:?} with {:?} txs",
                    batch_id,
                    batch_duration,
                    transactions.len()
                );
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
                    execution_duration: batch_duration,
                })
            }
        };

        self.current_execution_tasks
            .push(self.runtime.spawn(future));
        Ok(())
    }

    /// Report a batch's measured timings back to its producer (the txpool lane
    /// scheduler) if it requested feedback, then clear this batch's per-batch
    /// bookkeeping (`batch_feedback` + `batch_preparations`). Every
    /// batch-completion path — the main-loop [`Self::register_execution_result`],
    /// the end-of-block drain in [`Self::wait_all_execution_tasks`], and the
    /// [`Self::sequential_fallback`] path — routes through here, so no feedback
    /// handle is silently dropped and no map entry leaks to scheduler drop.
    /// Fire-at-most-once: a batch id with no pending handle (already reported, or
    /// the lane scheduler is disabled) is a no-op.
    ///
    /// `completed` selects the two semantics the lane scheduler distinguishes
    /// (see `lane-scheduler`'s `apply_feedback`, which feeds the overhead EMA on
    /// *every* report but only promotes the batch's in-pool children when
    /// `completed`):
    /// * `true`  — this batch's results are KEPT (they become part of the block).
    ///   Feeds the EMA and completes the batch's txs (idempotent with the
    ///   on-chain `RemovalReason::Committed` promotion path).
    /// * `false` — this batch executed but its results are DISCARDED and
    ///   re-executed sequentially ([`Self::sequential_fallback`]). The overhead
    ///   and inner-execution time genuinely happened, so we still feed them to
    ///   the EMA (this is exactly the overhead-floor signal that was being
    ///   starved), but we must NOT signal completion: the discarded batch never
    ///   committed, and its children are promoted by the commit path instead.
    ///   `completed: false` is therefore the safe, idempotent signal for
    ///   discarded work.
    fn report_batch_feedback(
        &mut self,
        batch_id: usize,
        execution_duration: Duration,
        completed: bool,
    ) {
        if let Some(feedback) = self.batch_feedback.remove(&batch_id) {
            feedback.handle.report(BatchExecutionReport {
                execution_time: duration_as_u64_nanos(execution_duration),
                overhead_time: duration_as_u64_nanos(feedback.overhead),
                completed,
            });
        }
        // Drop any leftover preparation stats for this batch so the map does not
        // leak entries past block end (metrics-only; `None` when metrics off).
        if let Some(batch_preparations) = self.batch_preparations.as_mut() {
            batch_preparations.remove(&batch_id);
        }
    }

    fn register_execution_result(&mut self, res: WorkSessionExecutionResult) {
        for contract in res.contracts_used.iter() {
            self.current_executing_contracts.remove(contract);
        }

        // Merge side of the per-contract `Changes` handoff: re-insert this
        // batch's per-contract changes into the shared map. Measured here so the
        // cost can be attributed to this batch's overhead before we report.
        let merge_start = Instant::now();
        for (contract_id, changes) in res.changes_per_contract {
            debug_assert!(!self.contracts_changes.contains_key(&contract_id));
            self.contracts_changes.insert(contract_id, changes);
        }
        let merge_handoff_duration = merge_start.elapsed();
        if self.config.metrics {
            parallel_executor_metrics::record_contract_handoff_merge(
                merge_handoff_duration,
            );
            self.time_accounting.handoff = self
                .time_accounting
                .handoff
                .saturating_add(merge_handoff_duration);
            // Worker occupancy: this batch's inner execution time is worker-seconds
            // spent. Covers the main-loop and drain paths (both route here).
            self.time_accounting.worker_busy = self
                .time_accounting
                .worker_busy
                .saturating_add(res.execution_duration);
        }
        if let Some(pending) = self.batch_feedback.get_mut(&res.batch_id) {
            pending.overhead = pending.overhead.saturating_add(merge_handoff_duration);
        }

        // Report this completed batch's measured timings back to the producer
        // (the txpool lane scheduler). `execution_time` is the inner batch work;
        // `overhead_time` is the batch's full parallelization overhead — batch
        // preparation + the split + merge per-contract handoff (accumulated into
        // the pending feedback at each of those sites). `completed: true` — this
        // batch's results are kept.
        self.report_batch_feedback(res.batch_id, res.execution_duration, true);

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

    // Returns `(exceeded_deadline, fallback_next_start_idx)`. The second element
    // is `Some(next_idx)` if a fallback ran during the drain (so the caller can
    // reset its block-tx counter for the blob stage / metrics), else `None`.
    async fn wait_all_execution_tasks(
        &mut self,
        storage_with_da: Arc<StorageTransaction<View>>,
    ) -> Result<(bool, Option<u32>), SchedulerError> {
        let mut fallback_next_start_idx = None;
        // We have reached the deadline
        // We need to merge the states of all the workers
        while !self.current_execution_tasks.is_empty() {
            match self.current_execution_tasks.next().await {
                Some(Ok(res)) => {
                    let res = res?;
                    if !res.skipped_tx.is_empty() {
                        drop(res.worker_id);
                        let next_start_idx = self
                            .sequential_fallback(
                                res.batch_id,
                                res.txs,
                                res.coins_used,
                                res.coins_created,
                                res.message_nonces_used,
                                res.execution_duration,
                                storage_with_da.clone(),
                            )
                            .await?;
                        fallback_next_start_idx = Some(next_start_idx);
                        break;
                    } else {
                        // End-of-block drain: this batch completed cleanly and
                        // its results are kept. Route through the SAME
                        // registration as the main loop so nothing is dropped.
                        //
                        // FIX 1 (consensus bug): this path used to insert only the
                        // batch's non-contract `Changes` into `execution_results`
                        // and NEVER re-inserted its `changes_per_contract` into the
                        // shared `contracts_changes` map (which `execute_batch`
                        // removed at dispatch). The final
                        // `verify_coherency_and_merge_results` merge folds
                        // `contracts_changes`, so a batch completing here silently
                        // dropped ALL its per-contract writes (e.g.
                        // `ContractsLatestUtxo`) from the block — a producer/
                        // validator state split. `register_execution_result`
                        // re-inserts the per-contract changes (and also reports
                        // `completed: true`, records the merge-handoff metric, and
                        // frees the batch's contracts + gas), so delegating to it
                        // fixes the drop and removes the divergence from the main
                        // loop.
                        self.register_execution_result(res);
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
        let mut exceeded_deadline = false;
        if now > self.deadline {
            tracing::warn!(
                "Execution time exceeded the limit by: {}ms",
                now.checked_duration_since(self.deadline)
                    .expect("Checked above; qed")
                    .as_millis()
            );
            exceeded_deadline = true;
        }
        Ok((exceeded_deadline, fallback_next_start_idx))
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
        let mut compiled_created_coins =
            CoinDependencyChainVerifier::new(self.config.utxo_validation);
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

    // Wait for all the workers to finish, gather all their transactions and
    // re-execute the affected contiguous batch-id range on a single worker
    // without the skipped transaction. We also fetch every already-executed
    // batch inside that range so the replay covers the whole `[lower, higher]`
    // window in committed order.
    //
    // Correctness of the replay hinges on the STARTING view. The re-execution
    // must observe exactly the state left by the KEPT earlier batches (id
    // `< lower`) — otherwise a range tx that reads a contract mutated by a kept
    // batch, or spends a coin/message it created, would replay against stale
    // state and diverge from the sequential (validation) executor, which is a
    // consensus split. We therefore seed the replay with:
    //   * `storage_with_da` — the pre-block view PLUS the DA-import changes
    //     (previously the replay used a bare `latest_view()`, dropping DA), and
    //   * the per-contract state as of `lower`, rebuilt from
    //     `dispatched_contract_seeds` (the seed each in-range batch was handed;
    //     the lowest-id toucher of a contract carries its as-of-`lower` state).
    // Coins/messages created by kept batches need no seeding: the executor runs
    // with `forbid_fake_utxo: false`, so a spend of a not-yet-committed
    // input succeeds and the post-hoc `CoinDependencyChainVerifier` / nonce
    // dedup validate it, and the canonical fold nets create-then-spend to a
    // `Remove` (matching the sequential map). Contract state has no such
    // post-hoc check, which is why it must be seeded.
    //
    // The replay runs through `InMemoryTransactionWithContracts` (as normal
    // batches do), so its contract writes are split back out per contract and
    // merged into `contracts_changes` (one entry per contract, preserving the
    // canonical-fold invariant); only the non-contract (coin/message) writes go
    // into this batch's `ChangesList` entry.
    //
    // Tell the TransactionSource that this transaction is skipped
    // to avoid sending new transactions that depend on it (using preconfirmation squeeze out)
    //
    // Can be replaced by a mechanism that replace the skipped_tx by a dummy transaction to not shift everything
    // TODO: Rework this function to continue the execution from the batch that got conflict
    //  instead of re-executing the whole `[lower, higher]` range.
    #[allow(clippy::too_many_arguments)]
    async fn sequential_fallback(
        &mut self,
        batch_id: usize,
        txs: Vec<Transaction>,
        coins_used: Vec<CoinInBatch>,
        coins_created: Vec<CoinInBatch>,
        message_nonces_used: Vec<Nonce>,
        execution_duration: Duration,
        storage_with_da: Arc<StorageTransaction<View>>,
    ) -> Result<u32, SchedulerError> {
        let fallback_start = Instant::now();
        let block_height = *self.header_to_produce.height();
        // This batch's parallel results are discarded and re-executed
        // sequentially below. Report its feedback as `completed: false` — the
        // measured overhead/exec time still feeds the lane scheduler's overhead
        // EMA, but the batch did not commit, so it must not signal completion.
        // Also clears its `batch_feedback` / `batch_preparations` bookkeeping.
        self.report_batch_feedback(batch_id, execution_duration, false);
        if self.config.metrics {
            // The discarded work still consumed worker-seconds.
            self.time_accounting.worker_busy = self
                .time_accounting
                .worker_busy
                .saturating_add(execution_duration);
        }
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
                    // Every other in-flight batch is also consumed and
                    // re-executed here, so it is discarded too: same
                    // `completed: false` semantics + bookkeeping cleanup.
                    self.report_batch_feedback(
                        res.batch_id,
                        res.execution_duration,
                        false,
                    );
                    if self.config.metrics {
                        self.time_accounting.worker_busy = self
                            .time_accounting
                            .worker_busy
                            .saturating_add(res.execution_duration);
                    }
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

        // Rebuild the per-contract state as of `lower_batch_id`. For each
        // contract touched anywhere in the range, take the seed handed to the
        // LOWEST-id in-range batch that touched it: that batch saw exactly the
        // state left by the kept batches (< lower), because any earlier in-range
        // batch that did not touch the contract left it unchanged. Higher-id
        // seeds also fold in earlier in-range writes, which the replay itself
        // recomputes — so first-writer-wins per contract is precisely the
        // as-of-`lower` state. Contracts first created/touched inside the range
        // have no seed here and correctly start from the base view.
        let mut contract_seed: FxHashMap<ContractId, Changes> = FxHashMap::default();
        for id in lower_batch_id..=higher_batch_id {
            if let Some(seed) = self.dispatched_contract_seeds.remove(&id) {
                for (contract_id, changes) in seed {
                    contract_seed.entry(contract_id).or_insert(changes);
                }
            }
        }

        // Replay the range starting at the block index the lowest-id batch in
        // the range originally used, so the replayed txs keep their true block
        // positions (their `TxPointer` must match the sequential executor's, or
        // e.g. `ContractsLatestUtxo` diverges). All batches before `lower` are
        // kept and committed, so this equals the count of block txs before the
        // range.
        let start_idx_txs = self
            .dispatched_start_idx
            .get(&lower_batch_id)
            .copied()
            .unwrap_or(0);

        let executor = self.executor.clone();
        // Get a memory instance for the re-execution
        let mut memory_instance = self.memory_pool.take_raw();
        // Replay over the pre-block view + DA changes, seeded with the
        // as-of-`lower` per-contract state, using the same
        // contract-splitting transaction the parallel batches use.
        let memory_tx =
            InMemoryTransactionWithContracts::new(storage_with_da, contract_seed);
        let mut storage_tx = StructuredStorage::new(memory_tx);
        let (transactions, mut execution_data) = executor
            .execute_l2_transactions(
                Components {
                    header_to_produce: self.header_to_produce,
                    transactions_source: OnceTransactionsSource::new(all_txs),
                    coinbase_recipient: self.coinbase_recipient,
                    gas_price: self.gas_price,
                },
                &mut storage_tx,
                start_idx_txs,
                memory_instance.as_mut(),
            )
            .await?;
        // Split the replay's writes: contract state → merged back into
        // `contracts_changes` (one entry per contract), everything else →
        // this batch's `ChangesList` entry.
        let (changes, changes_per_contract) = storage_tx.into_storage().into_changes();
        execution_data.changes = changes;
        for (contract_id, contract_changes) in changes_per_contract {
            // The range's contracts were removed from `contracts_changes` when
            // the in-range batches picked them up, so there is normally no
            // existing entry. Fold defensively if one is present so we never
            // emit two `ChangesList` entries for a single contract (which the
            // strict canonical fold would reject).
            if let Some(existing) = self.contracts_changes.remove(&contract_id) {
                let merged =
                    fold_changes_in_canonical_order(vec![existing, contract_changes])?;
                self.contracts_changes.insert(contract_id, merged);
            } else {
                self.contracts_changes.insert(contract_id, contract_changes);
            }
        }
        if !execution_data.skipped_transactions.is_empty() {
            let skipped = execution_data
                .skipped_transactions
                .iter()
                .map(|(tx_id, error)| format!("{tx_id}: {error}"))
                .collect::<Vec<_>>()
                .join("; ");
            eprintln!(
                "parallel executor sequential fallback skipped {} tx(s): {skipped}",
                execution_data.skipped_transactions.len()
            );
        }

        // Save execution results for all batch id with empty data
        // to not break the batch chain
        for id in lower_batch_id..=higher_batch_id {
            self.execution_results
                .insert(id, WorkSessionSavedData::default());
        }
        // The number of block txs committed through the end of this range: the
        // range's starting index plus what the replay actually committed. The
        // caller resets its running tx counter to this so subsequent batches
        // (and the blob stage) get contiguous `start_idx_txs` matching the
        // sequential validator — the fallback dropped the range's skipped txs,
        // which the dispatch-time counter had already counted.
        let committed_in_range = u32::try_from(transactions.len()).map_err(|_| {
            SchedulerError::InternalError("Too many transactions".to_string())
        })?;
        let next_start_idx_txs = start_idx_txs.saturating_add(committed_in_range);

        if self.config.metrics {
            let fallback_duration = fallback_start.elapsed();
            parallel_executor_metrics::record_sequential_fallback(fallback_duration);
            self.time_accounting.fallback = self
                .time_accounting
                .fallback
                .saturating_add(fallback_duration);
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
                skipped_tx: execution_data.skipped_transactions,
                used_gas: execution_data.used_gas,
                used_size: execution_data.used_size,
                coinbase: execution_data.coinbase,
            },
        );

        Ok(next_start_idx_txs)
    }
}

/// Fold a canonically-ordered list of [`Changes`] into a single [`Changes`],
/// coalescing per-key operations that the parallel executor split across
/// separate list entries.
///
/// The caller MUST pass the entries in canonical block order — DA-import
/// changes first (conceptually applied before batch 0), then each batch's
/// changes in batch-id order, then the per-contract changes. Applied in this
/// order the fold reproduces exactly what the SEQUENTIAL executor's single
/// coalescing map would compute for the same block, which is what keeps the
/// parallel *producer* and the sequential *validator* accepting the same
/// blocks.
///
/// ## Legal cross-entry sequences for a key (and the net op emitted)
/// * `[Insert]`         → `Insert`  — a coin/message/slot created (or a
///   pre-existing value overwritten) by a single entry.
/// * `[Remove]`         → `Remove`  — a pre-existing coin/message spent by a
///   single entry.
/// * `[Insert, Remove]` → `Remove`  — created *and* consumed inside this block:
///   a coin created in batch `i` and spent in batch `j > i`, or a DA-imported
///   message (`Insert` in `da_changes`, folded first) consumed by its one
///   in-block spender. The sequential map stores `Remove` for such a key
///   (`Insert` then `take`), so we emit `Remove`, not omission.
///
/// ## Genuine conflicts (rejected — the ordering does NOT legitimize them)
/// * `[Insert, Insert]` — two creations of the same key (e.g. two coins sharing
///   a `UtxoId`); impossible in a valid block.
/// * `[Remove, Remove]` — double spend of the same key.
/// * `[Remove, Insert]` — spend-before-create ordering.
/// * any longer sequence.
///
/// ## Why this stays correct for contract state
/// Per-contract state never reaches this fold split across entries: the
/// scheduler serialises every write to a given contract into that contract's
/// single `contracts_changes[c]` entry (guarded by
/// `current_executing_contracts`), so a slot written by several txs is already
/// coalesced *within one entry* and never appears here as a cross-entry
/// `[Insert, Insert]`. The one legitimate cross-entry contract overwrite — the
/// coinbase contract's UTXO/balance, re-written by the mint tx — is folded
/// separately and later (the mint runs last, against the merged view, with
/// `Overwrite`), so it is intentionally out of scope for this strict fold.
pub(crate) fn fold_changes_in_canonical_order(
    list: Vec<Changes>,
) -> Result<Changes, SchedulerError> {
    let mut acc = Changes::default();
    for changes in list {
        for (column, ops) in changes {
            let acc_column = acc.entry(column).or_default();
            for (key, op) in ops {
                match acc_column.entry(key) {
                    btree_map::Entry::Vacant(vacant) => {
                        vacant.insert(op);
                    }
                    btree_map::Entry::Occupied(mut occupied) => {
                        // The only legal collision is create-then-spend
                        // (`Insert` already recorded, now superseded by a
                        // `Remove`): net `Remove`, matching the sequential map.
                        let legal = matches!(
                            (occupied.get(), &op),
                            (WriteOperation::Insert(_), WriteOperation::Remove)
                        );
                        if legal {
                            occupied.insert(WriteOperation::Remove);
                        } else {
                            return Err(SchedulerError::InternalError(format!(
                                "Conflicting storage writes for column {column} key \
                                 {:?}: existing {:?} cannot be followed by {:?}",
                                occupied.key(),
                                occupied.get(),
                                op,
                            )));
                        }
                    }
                }
            }
        }
    }
    Ok(acc)
}

/// Convert a [`Duration`] into `u64` nanoseconds, saturating (durations that
/// large do not occur in block production; this only guards the cast).
fn duration_as_u64_nanos(duration: Duration) -> u64 {
    u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX)
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

fn format_tx_ids(txs: impl IntoIterator<Item = TxId>) -> String {
    txs.into_iter()
        .map(|tx_id| tx_id.to_string())
        .collect::<Vec<_>>()
        .join(",")
}
