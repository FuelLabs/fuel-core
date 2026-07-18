//! A [`TransactionsSource`] over a RECEIVED block, used for parallel block
//! VALIDATION.
//!
//! The source seeds a local [`lane_scheduler::LaneScheduler`] with the block's
//! L2 transactions (in block order) and answers the scheduler's asks with
//! conflict-free batches, exactly like the txpool's lane integration does for
//! block production (see `fuel-core-txpool`'s `lane_integration` module, the
//! source of truth for the access-derivation rule mirrored here).
//!
//! Differences from the production pool source:
//! * The transaction set is CLOSED (the block): every transaction is admitted
//!   with `tip = 0` — priority is meaningless during validation, everything is
//!   executed eventually.
//! * Each batch carries the EXPLICIT block positions of its transactions
//!   ([`crate::ports::ExecutableBatch::execution_indices`]) so their
//!   `TxPointer`s land at the received block's indices.
//! * Batch-completion feedback is applied directly to the local lane scheduler
//!   (releasing in-block children whose parents completed) and pings the
//!   new-transactions notifier so the scheduler's run loop re-asks.

use crate::ports::{
    BatchFeedbackHandle,
    ExecutableBatch,
    Filter,
    TransactionFiltered,
    TransactionSourceExecutableTransactions,
    TransactionsSource,
};
use fuel_core_executor::ports::MaybeCheckedTransaction;
use fuel_core_types::{
    blockchain::transaction::TransactionExt,
    fuel_tx::{
        ConsensusParameters,
        ContractId,
        Input,
        Output,
        Transaction,
        TxId,
        UniqueIdentifier,
    },
    services::executor::{
        Error as ExecutorError,
        Result as ExecutorResult,
    },
};
use lane_scheduler::{
    Access,
    BatchFeedback,
    BatchRequest,
    ExecutingContracts,
    LaneScheduler,
    ScheduledTransaction,
    SchedulerConfig,
    WindowContext,
    WorkerBudget,
};
use std::{
    collections::{
        HashMap,
        HashSet,
    },
    sync::{
        Arc,
        Mutex,
    },
};
use tokio::sync::watch;

/// The received block's transaction as seen by the local lane scheduler.
#[derive(Debug)]
struct ValidationScheduledTx {
    id: TxId,
    max_gas: u64,
    size: u64,
    /// Pre-derived contract accesses (see [`derive_contract_accesses_from_io`]).
    accesses: Vec<(ContractId, Access)>,
    /// In-block coin parents: ids of EARLIER transactions in the same block
    /// whose outputs this transaction spends.
    parents: Vec<TxId>,
}

impl ScheduledTransaction for ValidationScheduledTx {
    type Id = TxId;
    type ContractId = ContractId;

    fn id(&self) -> TxId {
        self.id
    }

    fn max_gas(&self) -> u64 {
        self.max_gas
    }

    fn tip(&self) -> u64 {
        // Priority is meaningless during validation: every block transaction is
        // admitted (order falls back to admission order, then id).
        0
    }

    fn size(&self) -> u64 {
        self.size
    }

    fn contract_accesses(&self) -> impl Iterator<Item = (ContractId, Access)> + '_ {
        self.accesses.iter().copied()
    }

    fn parents(&self) -> impl Iterator<Item = TxId> + '_ {
        self.parents.iter().copied()
    }
}

/// Apply the fuel-core Read/Write derivation rule to a transaction's I/O.
///
/// COPY of `fuel-core-txpool`'s `lane_integration::derive_contract_accesses_from_io`
/// (the source of truth — keep in sync): a contract INPUT with a matching
/// `Output::Contract` is a `Write`; a contract INPUT without one is a `Read`;
/// a newly created contract (`Output::ContractCreated`) is a `Write` on the
/// fresh id.
fn derive_contract_accesses_from_io(
    inputs: &[Input],
    outputs: &[Output],
) -> Vec<(ContractId, Access)> {
    // Input indices that have a matching `Output::Contract` — those inputs are
    // written. `Output::input_index()` is `Some` only for `Output::Contract`.
    let mut written_inputs: HashSet<u16> = HashSet::new();
    for output in outputs {
        if let Some(idx) = output.input_index() {
            written_inputs.insert(idx);
        }
    }

    let mut accesses = Vec::new();
    for (index, input) in inputs.iter().enumerate() {
        if let Input::Contract(contract) = input {
            let access = if written_inputs.contains(&(index as u16)) {
                Access::Write
            } else {
                Access::Read
            };
            accesses.push((contract.contract_id, access));
        }
    }

    // Newly created contracts are writes on the fresh contract id.
    for output in outputs {
        if let Output::ContractCreated { contract_id, .. } = output {
            accesses.push((*contract_id, Access::Write));
        }
    }

    accesses
}

/// A block transaction awaiting dispatch.
struct PendingTx {
    tx: Transaction,
    /// Position of the transaction in the RECEIVED block (counted from after
    /// any L1-processed prefix; the mint is not part of the source).
    block_index: u32,
}

struct Inner {
    lane: LaneScheduler<ValidationScheduledTx>,
    /// Not-yet-dispatched transactions keyed by id. Emptied as batches are
    /// handed out; every transaction is dispatched exactly once.
    pending: HashMap<TxId, PendingTx>,
}

/// See the module docs.
pub struct ValidationTransactionsSource {
    inner: Arc<Mutex<Inner>>,
    /// Pinged on every batch-completion feedback so the scheduler's run loop
    /// re-asks once a lane frees. Kept alive for the whole validation run (the
    /// run loop's early exit is driven by its exhausted transaction budget, not
    /// by this channel closing).
    notify: Arc<watch::Sender<()>>,
}

impl ValidationTransactionsSource {
    /// Build the source from the received block's L2 transactions in BLOCK
    /// order. `first_block_index` is the block position of `transactions[0]`
    /// (the number of L1-processed transactions preceding it; `0` when the
    /// block has no L1 prefix).
    pub fn new(
        transactions: &[Transaction],
        first_block_index: u32,
        consensus_parameters: &ConsensusParameters,
    ) -> ExecutorResult<Self> {
        let chain_id = consensus_parameters.chain_id();

        // First pass: block position of every transaction id, for the in-block
        // coin-parent derivation below.
        let mut position_by_id: HashMap<TxId, usize> =
            HashMap::with_capacity(transactions.len());
        for (index, tx) in transactions.iter().enumerate() {
            position_by_id.insert(tx.id(&chain_id), index);
        }

        let mut lane = LaneScheduler::new(SchedulerConfig {
            // Mirrors the txpool's lane integration: the per-request
            // `BatchRequest::workers` is the ground truth for the answer shape;
            // this only seeds analytic slice sizing.
            workers: 1,
            block_gas_limit: Some(consensus_parameters.block_gas_limit()),
            ..SchedulerConfig::default()
        });
        let mut pending = HashMap::with_capacity(transactions.len());

        for (index, tx) in transactions.iter().enumerate() {
            let id = tx.id(&chain_id);
            let inputs = tx.inputs();
            let outputs = tx.outputs();
            let accesses = derive_contract_accesses_from_io(&inputs, &outputs);

            // In-block coin dependencies: inputs spending a UtxoId whose tx_id
            // belongs to an EARLIER transaction of this same block. A spend of
            // a LATER transaction's output is not a schedulable dependency —
            // the block is invalid, and executing the spender first surfaces
            // that as a validation failure (matching sequential).
            let mut parents = Vec::new();
            for input in inputs.iter() {
                if let Some(utxo_id) = input.utxo_id() {
                    if let Some(parent_index) = position_by_id.get(utxo_id.tx_id()) {
                        if *parent_index < index {
                            let parent_id = *utxo_id.tx_id();
                            if !parents.contains(&parent_id) {
                                parents.push(parent_id);
                            }
                        }
                    }
                }
            }

            let max_gas = tx.max_gas(consensus_parameters)?;
            let size = tx.size() as u64;
            let block_index = first_block_index
                .checked_add(u32::try_from(index).map_err(|_| {
                    ExecutorError::Other("too many transactions in block".to_string())
                })?)
                .ok_or_else(|| {
                    ExecutorError::Other("block index overflow".to_string())
                })?;

            lane.on_transaction(Arc::new(ValidationScheduledTx {
                id,
                max_gas,
                size,
                accesses,
                parents,
            }));
            pending.insert(
                id,
                PendingTx {
                    tx: tx.clone(),
                    block_index,
                },
            );
        }

        let (notify, _initial_rx) = watch::channel(());
        Ok(Self {
            inner: Arc::new(Mutex::new(Inner { lane, pending })),
            notify: Arc::new(notify),
        })
    }

    /// One synchronous ask against the local lane scheduler (the async trait
    /// method delegates here; no awaits, so the returned future stays `Send`).
    fn next_batches(
        &self,
        gas_limit: u64,
        total_gas_limit: u64,
        tx_count_limit: u32,
        block_transaction_size_limit: u64,
        free_worker_count: usize,
        filter: Filter,
    ) -> anyhow::Result<TransactionSourceExecutableTransactions> {
        let mut inner = self.inner.lock().expect("Mutex poisoned");

        // Mirror `lane_integration::next_batches`: one WorkerBudget per free
        // worker, in-flight locks from the excluded set (conservatively Write),
        // window `Some` with no deadline and the block's remaining gas.
        let mut executing_contracts = ExecutingContracts::new();
        for contract in filter.excluded_contract_ids.iter() {
            executing_contracts.lock(*contract, Access::Write);
        }
        let request = BatchRequest {
            workers: vec![
                WorkerBudget {
                    gas: gas_limit,
                    size: block_transaction_size_limit,
                    tx_count: u64::from(tx_count_limit),
                };
                free_worker_count
            ],
            executing_contracts,
            // Feedback is applied directly via `on_batch_feedback` in the
            // batch's feedback handle, not piggybacked on requests.
            feedback: Vec::new(),
            window: Some(WindowContext {
                now: 0,
                block_gas_remaining: total_gas_limit,
                window_fit_gas: gas_limit,
                deadline: None,
                block_start: 0,
            }),
        };
        let proposals = inner.lane.next_batches(&request);

        let mut batches = Vec::with_capacity(proposals.len());
        for proposal in proposals {
            inner.lane.on_dispatched(proposal.batch_id, &proposal.txs);

            let mut transactions = Vec::with_capacity(proposal.txs.len());
            let mut execution_indices = Vec::with_capacity(proposal.txs.len());
            for tx_id in &proposal.txs {
                let pending = inner.pending.remove(tx_id).ok_or_else(|| {
                    anyhow::anyhow!(
                        "lane scheduler proposed unknown or already-dispatched \
                         transaction {tx_id}"
                    )
                })?;
                execution_indices.push(pending.block_index);
                // Validation executes UNCHECKED transactions (full checks at
                // execution time), exactly like the sequential validator.
                transactions.push(MaybeCheckedTransaction::Transaction(pending.tx));
            }

            let batch_id = proposal.batch_id;
            let inner_for_feedback = Arc::clone(&self.inner);
            let notify = Arc::clone(&self.notify);
            let feedback_handle = BatchFeedbackHandle::new(move |report| {
                {
                    let mut inner = inner_for_feedback.lock().expect("Mutex poisoned");
                    inner.lane.on_batch_feedback(BatchFeedback {
                        batch_id,
                        overhead_time: report.overhead_time,
                        execution_time: report.execution_time,
                        completed: report.completed,
                    });
                }
                // A completed batch may have released in-block children — wake
                // the run loop so it re-asks. Ignore a closed channel (the run
                // loop already exited).
                let _ = notify.send(());
            });

            batches.push(ExecutableBatch {
                transactions,
                anchor_contract_ids: Vec::new(),
                feedback_handle: Some(feedback_handle),
                execution_indices: Some(execution_indices),
            });
        }

        Ok(TransactionSourceExecutableTransactions {
            batches,
            filtered: TransactionFiltered::NotFiltered,
            // Echo the in-flight lock set back: the scheduler re-derives it per
            // ask from its own dispatched-batch bookkeeping.
            filter,
            // The lane scheduler answers the WHOLE ask at once — fewer batches
            // than requested workers means nothing more is schedulable until a
            // batch completes.
            answered_all_workers: true,
        })
    }

    /// Number of transactions not yet handed out.
    #[cfg(test)]
    fn pending_count(&self) -> usize {
        self.inner.lock().expect("Mutex poisoned").pending.len()
    }
}

impl TransactionsSource for ValidationTransactionsSource {
    async fn get_executable_transactions(
        &self,
        gas_limit: u64,
        total_gas_limit: u64,
        tx_count_limit: u32,
        block_transaction_size_limit: u64,
        _selection_worker_count: usize,
        free_worker_count: usize,
        filter: Filter,
    ) -> anyhow::Result<TransactionSourceExecutableTransactions> {
        self.next_batches(
            gas_limit,
            total_gas_limit,
            tx_count_limit,
            block_transaction_size_limit,
            free_worker_count,
            filter,
        )
    }

    fn get_new_transactions_notifier(&self) -> watch::Receiver<()> {
        self.notify.subscribe()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::BatchExecutionReport;
    use fuel_core_types::{
        fuel_asm::op,
        fuel_tx::{
            Address,
            AssetId,
            TransactionBuilder,
            UtxoId,
        },
    };

    fn params() -> ConsensusParameters {
        ConsensusParameters::default()
    }

    fn coin_tx(rng_byte: u8, parent: Option<(TxId, u16)>) -> Transaction {
        let mut builder =
            TransactionBuilder::script(vec![op::ret(0)].into_iter().collect(), vec![]);
        builder.max_fee_limit(0).script_gas_limit(10_000);
        let utxo_id = match parent {
            Some((tx_id, output_index)) => UtxoId::new(tx_id, output_index),
            None => UtxoId::new([rng_byte; 32].into(), 0),
        };
        builder.add_unsigned_coin_input(
            fuel_core_types::fuel_crypto::SecretKey::try_from(
                [rng_byte.max(1); 32].as_slice(),
            )
            .expect("valid secret"),
            utxo_id,
            1_000_000,
            AssetId::BASE,
            Default::default(),
        );
        builder.add_output(Output::coin(Address::zeroed(), 500_000, AssetId::BASE));
        builder.finalize_as_transaction()
    }

    fn ask(
        source: &ValidationTransactionsSource,
        workers: usize,
    ) -> TransactionSourceExecutableTransactions {
        source
            .next_batches(
                u64::MAX / 4,
                u64::MAX / 4,
                u32::MAX,
                u64::MAX / 4,
                workers,
                Filter::new(HashSet::new()),
            )
            .expect("ask must succeed")
    }

    fn complete(batch: ExecutableBatch) {
        batch
            .feedback_handle
            .expect("validation batches always carry a feedback handle")
            .report(BatchExecutionReport {
                execution_time: 1,
                overhead_time: 1,
                completed: true,
            });
    }

    #[test]
    fn dispatches_every_transaction_exactly_once_with_block_indices() {
        let params = params();
        let txs: Vec<Transaction> = (1..=4u8).map(|i| coin_tx(i, None)).collect();
        let source = ValidationTransactionsSource::new(&txs, 0, &params).expect("source");

        let mut seen = HashMap::new();
        let mut rounds = 0usize;
        while source.pending_count() > 0 {
            rounds = rounds.saturating_add(1);
            assert!(rounds < 100, "source failed to drain");
            let answer = ask(&source, 4);
            for batch in answer.batches {
                let indices = batch
                    .execution_indices
                    .clone()
                    .expect("validation batches carry explicit indices");
                assert_eq!(indices.len(), batch.transactions.len());
                for (tx, index) in batch.transactions.iter().zip(&indices) {
                    let id = tx.id(&params.chain_id());
                    assert!(
                        seen.insert(id, *index).is_none(),
                        "transaction dispatched twice"
                    );
                }
                complete(batch);
            }
        }

        // Every tx dispatched with its block position.
        assert_eq!(seen.len(), txs.len());
        for (index, tx) in txs.iter().enumerate() {
            assert_eq!(seen[&tx.id(&params.chain_id())], index as u32);
        }
    }

    #[test]
    fn in_block_coin_child_waits_for_its_parent_batch() {
        let params = params();
        let parent = coin_tx(1, None);
        let parent_id = parent.id(&params.chain_id());
        let child = coin_tx(2, Some((parent_id, 0)));
        let child_id = child.id(&params.chain_id());
        let txs = vec![parent, child];
        let source = ValidationTransactionsSource::new(&txs, 0, &params).expect("source");

        // First ask: only the parent is ready.
        let answer = ask(&source, 2);
        let dispatched: Vec<TxId> = answer
            .batches
            .iter()
            .flat_map(|b| b.transactions.iter().map(|tx| tx.id(&params.chain_id())))
            .collect();
        assert!(dispatched.contains(&parent_id));
        assert!(!dispatched.contains(&child_id), "child released too early");
        for batch in answer.batches {
            complete(batch);
        }

        // After the parent's completion feedback, the child becomes ready.
        let answer = ask(&source, 2);
        let dispatched: Vec<TxId> = answer
            .batches
            .iter()
            .flat_map(|b| b.transactions.iter().map(|tx| tx.id(&params.chain_id())))
            .collect();
        assert!(dispatched.contains(&child_id));
        assert_eq!(source.pending_count(), 0);
    }
}
