//! A [`TransactionsSource`] over a RECEIVED block, used for parallel block
//! VALIDATION.
//!
//! Validation is a different scheduling problem from production, and this
//! source uses a different algorithm for it: [`lane_scheduler::ValidationScheduler`].
//!
//! Production CHOOSES a block's contents and order, so it ranks by fee rate and
//! whatever order the workers happen to finish in BECOMES the block's order.
//! Validation REPLAYS a block that already fixed both, and every transaction
//! must observe exactly the contract state its position implies — the executor
//! derives `TxPointer`s from that position, and a contract input carries the
//! latest utxo id and pointer of the contract it touches.
//!
//! Ordering by rank was not enough for that. A rank only orders what is
//! ELIGIBLE at ask time, so an earlier transaction blocked on one contract
//! loses its place to a later one that happens to be free — measured on real
//! block 5026 as a contract READER observing a writer that the block places
//! after it. The validation scheduler instead builds the block's dependency
//! graph once (write-after-write, read-after-write AND write-after-read) and
//! hands out batches that cannot violate it, so "the state its position
//! implies" holds by construction rather than by hope.
//!
//! Properties this source relies on:
//! * A batch's positions come out in ASCENDING block order, and a batch
//!   executes sequentially inside its worker, so a batch reproduces sequential
//!   semantics for the transactions it holds.
//! * Positions need not be contiguous: a worker may be handed 1, 3, 5 and 10
//!   because they form one contract's chain. The executor supports that through
//!   [`crate::ports::ExecutableBatch::execution_indices`].
//! * Concurrent READERS of a contract stay concurrent — the reason accesses are
//!   modelled as Read/Write rather than all-Write.
//! * NO in-block coin parents are edges: a coin child does not need its parent
//!   to have executed (see `new`). Coin ordering is enforced by block index in
//!   `CoinDependencyChainVerifier` and, within a batch, by ascending positions.

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
        UniqueIdentifier,
    },
    services::executor::{
        Error as ExecutorError,
        Result as ExecutorResult,
    },
};
use lane_scheduler::{
    Access,
    PlanBudget,
    ValidationCost,
    ValidationRequest,
    ValidationScheduler,
    ValidationTx,
};
use std::{
    collections::HashSet,
    sync::{
        Arc,
        Mutex,
        atomic::{
            AtomicU64,
            Ordering,
        },
    },
};
use tokio::sync::watch;

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

/// The executor's measured per-batch overhead, expressed in GAS so it can be
/// compared against transaction gas, and carried ACROSS blocks.
///
/// The validation scheduler plans the whole block before the first batch runs,
/// so unlike the production scheduler it cannot learn the overhead during the
/// block it is sizing — it needs a figure up front. Each completed batch
/// reports its overhead and execution time in nanoseconds, and the batch's own
/// declared gas converts one into the other:
///
/// ```text
/// overhead_gas = overhead_ns * batch_gas / execution_ns
/// ```
///
/// The running mean of that lands here and seeds the NEXT block. Zero (the
/// starting value, before any batch has ever been measured) makes the
/// scheduler plan one transaction per batch, which is the correct answer when
/// batching genuinely saves nothing.
#[derive(Debug, Default)]
pub struct ValidationOverheadEstimate {
    gas: AtomicU64,
}

impl ValidationOverheadEstimate {
    /// The current estimate, for seeding a block's plan.
    pub fn get(&self) -> u64 {
        self.gas.load(Ordering::Relaxed)
    }

    fn set(&self, gas: u64) {
        self.gas.store(gas, Ordering::Relaxed);
    }
}

/// Per-block ask accounting, to tell the two causes of an idle worker apart.
///
/// A worker idles either because the PLAN has nothing dispatchable for it (the
/// dependency wavefront is narrow — structural, and the plan's own makespan
/// already accounts for it), or because it was dispatchable and the worker did
/// not get it promptly (ask cadence / latency — invisible to the plan). The
/// source can only ever hand back `min(free workers, startable batches)`, so
/// `served < requested` is exactly the structural case and everything else is
/// latency.
#[derive(Debug, Default)]
struct AskStats {
    asks: u64,
    requested: u64,
    served: u64,
    /// Ask slots we could not fill because the plan had nothing startable.
    starved: u64,
    /// Asks that returned nothing at all.
    empty_asks: u64,
    /// Largest startable set seen at ask time — how much slack the plan had.
    max_startable: usize,
}

struct Inner {
    scheduler: ValidationScheduler,
    asks: AskStats,
    /// Block transactions by position offset from `first_block_index`, taken
    /// as they are dispatched. Every transaction is dispatched exactly once.
    pending: Vec<Option<Transaction>>,
    /// Declared gas per transaction, same indexing — used to convert a batch's
    /// measured overhead into gas.
    declared_gas: Vec<u64>,
}

/// See the module docs.
pub struct ValidationTransactionsSource {
    inner: Arc<Mutex<Inner>>,
    /// Block position of `transactions[0]`.
    first_block_index: u32,
    /// Pinged on every batch completion so the scheduler's run loop re-asks
    /// once a worker frees.
    notify: Arc<watch::Sender<()>>,
    /// Shared across blocks; see [`ValidationOverheadEstimate`].
    overhead: Arc<ValidationOverheadEstimate>,
}

impl ValidationTransactionsSource {
    /// Build the source from the received block's L2 transactions in BLOCK
    /// order. `first_block_index` is the block position of `transactions[0]`
    /// (the number of L1-processed transactions preceding it; `0` when the
    /// block has no L1 prefix). `worker_count` is the machine's width, which
    /// anchors the scheduler's makespan bound and load-balance target.
    pub fn new(
        transactions: &[Transaction],
        first_block_index: u32,
        consensus_parameters: &ConsensusParameters,
        worker_count: usize,
        overhead: Arc<ValidationOverheadEstimate>,
    ) -> ExecutorResult<Self> {
        let chain_id = consensus_parameters.chain_id();
        let mut planned: Vec<ValidationTx<ContractId>> =
            Vec::with_capacity(transactions.len());
        let mut pending: Vec<Option<Transaction>> =
            Vec::with_capacity(transactions.len());
        let mut declared_gas: Vec<u64> = Vec::with_capacity(transactions.len());

        for (index, tx) in transactions.iter().enumerate() {
            let _ = tx.id(&chain_id);
            let inputs = tx.inputs();
            let outputs = tx.outputs();
            let accesses = derive_contract_accesses_from_io(&inputs, &outputs);

            // NO in-block coin dependencies become graph edges.
            //
            // A coin child does not need its parent to have executed: Fuel's
            // `Input::CoinSigned`/`CoinPredicate` DECLARE the coin's `owner`,
            // `amount` and `asset_id`, and the executor builds the coin from
            // those input fields (`get_coin_or_default`). The parallel path
            // already runs with `forbid_fake_utxo: false` precisely so batches
            // may execute against pre-block views where same-block coins do not
            // exist yet, with existence and equality checked afterwards by
            // `CoinDependencyChainVerifier`.
            //
            // What the block DOES require is that a coin only funds a LATER
            // transaction — an ordering fact fixed by the block, decided from
            // BLOCK INDICES by the verifier rather than from the order batches
            // happened to run in. Making it a scheduling edge as well only made
            // the child unschedulable until its parent's batch completed:
            // measured on real block 5026 as 4.79/8 average worker concurrency
            // with the edges versus 7.49/8 without.
            //
            // PRODUCTION keeps its coin-parent gating (see the txpool source):
            // there the block order is being CHOSEN rather than replayed, and a
            // child whose parent is skipped must not be included.
            //
            // Declared gas, not gas used: validation replays a block whose
            // receipts it has not computed yet, so the limit is the only figure
            // available. It is an upper bound, so batches come out no larger
            // than intended.
            let gas = tx.max_gas(consensus_parameters)?;
            let position = first_block_index
                .checked_add(u32::try_from(index).map_err(|_| {
                    ExecutorError::Other("too many transactions in block".to_string())
                })?)
                .ok_or_else(|| {
                    ExecutorError::Other("block index overflow".to_string())
                })?;

            planned.push(ValidationTx {
                position,
                gas,
                accesses,
            });
            declared_gas.push(gas);
            pending.push(Some(tx.clone()));
        }

        // Plan the whole block up front. The budget is NONE: refinement is a
        // local search that costs oracle simulations, and across the scenario
        // matrix it changes nothing at the overheads a real executor exhibits
        // while the portfolio alone already matches or beats what production
        // achieved building the block.
        let scheduler = ValidationScheduler::with_budget(
            &planned,
            ValidationCost {
                batch_overhead: overhead.get(),
                per_tx_overhead: 0,
            },
            worker_count,
            // Refinement recovers <=1% on real blocks (measured: the plan
            // sits ~4% above the work/8 bound and stays there under a 50k-eval
            // budget), so the whole-block portfolio plan is taken as-is.
            PlanBudget::NONE,
        );

        // THE BLOCK'S OWN CEILING, logged so a measured concurrency can be read
        // against what this block actually admits. `dag_max` is the best any
        // scheduler could reach with unlimited workers; if it is below the
        // worker count, the block — not the scheduler — is the limit.
        // `longest_contract_chain` separates the two ways that happens: a
        // single hot contract everything queues behind, versus a graph that is
        // long because paths HOP between contracts (an account's consecutive
        // orders landing on different orderbooks), which no single contract
        // explains.
        let stats = scheduler.stats;
        let bound = stats
            .critical_path
            .max(stats.total_work / (worker_count.max(1) as u64));
        let dag_max = if stats.critical_path > 0 {
            stats.total_work as f64 / stats.critical_path as f64
        } else {
            0.0
        };
        tracing::info!(
            target: "parallel_executor::validation_graph",
            txs = planned.len(),
            batches = scheduler.planned_len(),
            total_work = stats.total_work,
            critical_path = stats.critical_path,
            longest_contract_chain = stats.longest_contract_chain,
            dag_max_concurrency = dag_max,
            planned_with_overhead_gas = overhead.get(),
            // What the plan itself predicts, against what no schedule can
            // beat. plan_vs_bound near 1.0 means the PLAN is essentially
            // optimal and any shortfall is reality diverging from the cost
            // model; well above 1.0 means the scheduler is leaving time on
            // the table and the algorithm is what to improve.
            bound = bound,
            plan_span = stats.refined_span,
            plan_vs_bound = if bound > 0 {
                stats.refined_span as f64 / bound as f64
            } else {
                0.0
            },
            "validation dependency graph: the block's own parallelism ceiling",
        );

        let (notify, _initial_rx) = watch::channel(());
        Ok(Self {
            inner: Arc::new(Mutex::new(Inner {
                scheduler,
                asks: AskStats::default(),
                pending,
                declared_gas,
            })),
            first_block_index,
            notify: Arc::new(notify),
            overhead,
        })
    }

    /// One synchronous ask against the plan (the async trait method delegates
    /// here; no awaits, so the returned future stays `Send`).
    fn next_batches(
        &self,
        free_worker_count: usize,
        filter: Filter,
    ) -> anyhow::Result<TransactionSourceExecutableTransactions> {
        let mut inner = self.inner.lock().expect("Mutex poisoned");
        let startable = inner.scheduler.ready_len();
        let proposals = inner.scheduler.next_batches(&ValidationRequest {
            workers: free_worker_count,
        });
        {
            let served = proposals.len() as u64;
            let stats = &mut inner.asks;
            stats.asks = stats.asks.saturating_add(1);
            stats.requested = stats.requested.saturating_add(free_worker_count as u64);
            stats.served = stats.served.saturating_add(served);
            stats.starved = stats
                .starved
                .saturating_add((free_worker_count as u64).saturating_sub(served));
            if served == 0 {
                stats.empty_asks = stats.empty_asks.saturating_add(1);
            }
            stats.max_startable = stats.max_startable.max(startable);
        }
        {
            let drained = inner.scheduler.is_drained();
            let stats = &inner.asks;
            if drained {
                tracing::info!(
                    target: "parallel_executor::validation_asks",
                    asks = stats.asks,
                    requested = stats.requested,
                    served = stats.served,
                    starved = stats.starved,
                    starved_pct = 100.0 * stats.starved as f64
                        / (stats.requested.max(1)) as f64,
                    empty_asks = stats.empty_asks,
                    max_startable = stats.max_startable,
                    "validation ask accounting: structural starvation vs dispatch cadence",
                );
            }
        }

        let mut batches = Vec::with_capacity(proposals.len());
        for proposal in proposals {
            let mut transactions = Vec::with_capacity(proposal.positions.len());
            let mut execution_indices = Vec::with_capacity(proposal.positions.len());
            let mut batch_gas: u64 = 0;
            for position in &proposal.positions {
                let offset =
                    usize::try_from(position.saturating_sub(self.first_block_index))
                        .map_err(|_| {
                            anyhow::anyhow!("block position {position} out of range")
                        })?;
                let tx = inner
                    .pending
                    .get_mut(offset)
                    .and_then(Option::take)
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "validation scheduler proposed unknown or \
                             already-dispatched position {position}"
                        )
                    })?;
                batch_gas = batch_gas
                    .saturating_add(inner.declared_gas.get(offset).copied().unwrap_or(0));
                // Positions arrive ascending, which is what makes a batch's
                // sequential execution reproduce the block's own order.
                execution_indices.push(*position);
                // Validation executes UNCHECKED transactions (full checks at
                // execution time), exactly like the sequential validator.
                transactions.push(MaybeCheckedTransaction::Transaction(tx));
            }

            let batch_size = proposal.positions.len();
            let batch_id = proposal.batch_id;
            let inner_for_feedback = Arc::clone(&self.inner);
            let notify = Arc::clone(&self.notify);
            let overhead = Arc::clone(&self.overhead);
            let feedback_handle = BatchFeedbackHandle::new(move |report| {
                {
                    let mut inner = inner_for_feedback.lock().expect("Mutex poisoned");
                    // Convert this batch's measured overhead into gas via its
                    // own declared gas and execution time, and fold it into the
                    // running mean for the next block.
                    // Does real batch cost follow the plan's model? The plan
                    // charges a batch its members' DECLARED gas, which is a
                    // constant per transaction, so it predicts cost strictly
                    // proportional to batch size. Logging size against the
                    // measured time exposes any FIXED per-batch component the
                    // model is blind to.
                    tracing::debug!(
                        target: "parallel_executor::validation_batch",
                        n = batch_size,
                        execution_ns = report.execution_time,
                        overhead_ns = report.overhead_time,
                    );
                    if report.execution_time > 0 && batch_gas > 0 {
                        let overhead_gas = u128::from(report.overhead_time)
                            .saturating_mul(u128::from(batch_gas))
                            .checked_div(u128::from(report.execution_time))
                            .unwrap_or(0);
                        inner.scheduler.on_batch_feedback(
                            u64::try_from(overhead_gas).unwrap_or(u64::MAX),
                        );
                        overhead.set(inner.scheduler.measured_batch_overhead());
                    }
                    // Release whatever this batch was blocking. Done even for a
                    // batch reported as not completed: validation fails as a
                    // whole in that case, and holding the plan back would stall
                    // the run loop instead of surfacing the error.
                    inner.scheduler.on_batch_complete(batch_id);
                }
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
            filter,
            // The plan answers the whole ask at once — fewer batches than free
            // workers means nothing more is dispatchable until one completes.
            answered_all_workers: true,
        })
    }

    /// Number of transactions not yet handed out.
    #[cfg(test)]
    fn pending_count(&self) -> usize {
        self.inner
            .lock()
            .expect("Mutex poisoned")
            .pending
            .iter()
            .filter(|slot| slot.is_some())
            .count()
    }
}

impl TransactionsSource for ValidationTransactionsSource {
    async fn get_executable_transactions(
        &self,
        _gas_limit: u64,
        _total_gas_limit: u64,
        _tx_count_limit: u32,
        _block_transaction_size_limit: u64,
        _selection_worker_count: usize,
        free_worker_count: usize,
        filter: Filter,
    ) -> anyhow::Result<TransactionSourceExecutableTransactions> {
        // The per-worker budgets are SELECTION limits: production uses them to
        // decide what to include. Validation's set is already fixed and every
        // transaction must run, so the plan is sized by the block's own
        // structure and these are ignored.
        self.next_batches(free_worker_count, filter)
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
            TxId,
            UtxoId,
        },
    };
    use std::collections::HashMap;

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
            .next_batches(workers, Filter::new(HashSet::new()))
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
        let source = ValidationTransactionsSource::new(
            &txs,
            0,
            &params,
            4,
            Arc::new(ValidationOverheadEstimate::default()),
        )
        .expect("source");

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

    // A coin child is NOT gated on its parent during validation: both are
    // released in the SAME ask. The child's input declares the coin's owner,
    // amount and asset id, so it executes correctly whether or not its parent
    // has run; ordering is enforced where it actually matters — by block index
    // in `CoinDependencyChainVerifier`, and by block order WITHIN a batch (see
    // below). Gating them cost roughly a third of validation's worker time.
    #[test]
    fn in_block_coin_child_is_not_gated_on_its_parent() {
        let params = params();
        let parent = coin_tx(1, None);
        let parent_id = parent.id(&params.chain_id());
        let child = coin_tx(2, Some((parent_id, 0)));
        let child_id = child.id(&params.chain_id());
        let txs = vec![parent, child];
        let source = ValidationTransactionsSource::new(
            &txs,
            0,
            &params,
            4,
            Arc::new(ValidationOverheadEstimate::default()),
        )
        .expect("source");

        let answer = ask(&source, 2);
        let dispatched: Vec<TxId> = answer
            .batches
            .iter()
            .flat_map(|b| b.transactions.iter().map(|tx| tx.id(&params.chain_id())))
            .collect();
        assert!(
            dispatched.contains(&parent_id) && dispatched.contains(&child_id),
            "parent and child must both be schedulable immediately",
        );
        for batch in answer.batches {
            complete(batch);
        }
        assert_eq!(source.pending_count(), 0);
    }

    // Whatever order the scheduler proposes them in, a batch hands its
    // transactions over in BLOCK order — they execute sequentially inside the
    // worker, so any other order would apply their writes out of sequence (a
    // coin child spending before its parent creates, for instance) and diverge
    // from sequential execution.
    #[test]
    fn batches_are_handed_over_in_block_order() {
        let params = params();
        let txs: Vec<Transaction> = (1..=6u8).map(|i| coin_tx(i, None)).collect();
        let source = ValidationTransactionsSource::new(
            &txs,
            0,
            &params,
            4,
            Arc::new(ValidationOverheadEstimate::default()),
        )
        .expect("source");

        let mut batches_seen = 0;
        loop {
            let answer = ask(&source, 4);
            if answer.batches.is_empty() {
                break;
            }
            for batch in answer.batches {
                let indices = batch
                    .execution_indices
                    .clone()
                    .expect("validation batches carry explicit indices");
                let mut sorted = indices.clone();
                sorted.sort_unstable();
                assert_eq!(
                    indices, sorted,
                    "a batch must be handed over in block order",
                );
                batches_seen += 1;
                complete(batch);
            }
        }
        assert!(batches_seen > 0);
        assert_eq!(source.pending_count(), 0);
    }
}
