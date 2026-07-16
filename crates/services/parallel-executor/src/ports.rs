use fuel_core_executor::ports::MaybeCheckedTransaction;
use fuel_core_types::fuel_tx::ContractId;
use std::collections::HashSet;
use tokio::sync::watch;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransactionFiltered {
    /// Some transactions were filtered out and so could be fetched in the future
    Filtered,
    /// No transactions were filtered out
    NotFiltered,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Filter {
    /// The set of contract IDs to filter out
    pub excluded_contract_ids: HashSet<ContractId>,
}

impl Filter {
    pub fn new(excluded_contract_ids: HashSet<ContractId>) -> Self {
        Self {
            excluded_contract_ids,
        }
    }
}

/// Measured timings for a completed batch, reported back to whoever produced the
/// batch (today: the txpool lane scheduler). Time units are opaque to the
/// executor — the producer decides how to interpret them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BatchExecutionReport {
    /// Inner execution cost of the batch (the work itself).
    pub execution_time: u64,
    /// Parallelization overhead the executor paid for this batch (batch
    /// preparation / split / merge). Partial today — see the caller.
    pub overhead_time: u64,
    /// Whether the batch finished executing successfully.
    pub completed: bool,
}

/// Opaque, fire-at-most-once handle that travels alongside a batch of executable
/// transactions. Whoever registers the batch's execution result calls
/// [`Self::report`] exactly once with the measured timings; the handle then
/// forwards them to the batch's producer.
///
/// The executor never learns anything about the producer's internals (batch
/// ids, channels): the entire transport is captured inside an opaque closure, so
/// the parallel-executor crate stays independent of the txpool.
///
/// Dropping a handle without reporting is intentionally silent — the producer
/// (the lane scheduler) tolerates missing feedback by design, so a shutdown or
/// error path that loses a handle degrades gracefully rather than panicking.
pub struct BatchFeedbackHandle {
    report_fn: Option<Box<dyn FnOnce(BatchExecutionReport) + Send>>,
}

impl BatchFeedbackHandle {
    /// Build a handle from the producer-supplied reporting closure. The closure
    /// runs at most once, when [`Self::report`] is called.
    pub fn new(report_fn: impl FnOnce(BatchExecutionReport) + Send + 'static) -> Self {
        Self {
            report_fn: Some(Box::new(report_fn)),
        }
    }

    /// Report the batch's measured timings back to the producer, consuming the
    /// handle so it can fire at most once.
    pub fn report(mut self, report: BatchExecutionReport) {
        if let Some(report_fn) = self.report_fn.take() {
            report_fn(report);
        }
    }
}

impl core::fmt::Debug for BatchFeedbackHandle {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("BatchFeedbackHandle")
            .finish_non_exhaustive()
    }
}

pub struct TransactionSourceExecutableTransactions {
    /// The transactions that can be executed
    pub transactions: Vec<MaybeCheckedTransaction>,
    /// Anchor contracts selected by the tx pool while building this batch
    pub anchor_contract_ids: Vec<ContractId>,
    /// Indicates whether some transactions were filtered out based on the filter
    pub filtered: TransactionFiltered,
    /// The filter used to fetch these transactions
    pub filter: Filter,
    /// Opaque handle to report this batch's completion/timings back to the
    /// producer. `None` when the producer wants no feedback (e.g. the txpool
    /// lane scheduler is disabled) — the default, zero-overhead path.
    pub feedback_handle: Option<BatchFeedbackHandle>,
}

pub trait TransactionsSource {
    /// Returns the a batch of transactions to satisfy the given parameters
    fn get_executable_transactions(
        &self,
        gas_limit: u64,
        tx_count_limit: u32,
        block_transaction_size_limit: u64,
        selection_worker_count: usize,
        filter: Filter,
    ) -> impl Future<Output = anyhow::Result<TransactionSourceExecutableTransactions>>;

    /// Returns a notification receiver for new transactions
    fn get_new_transactions_notifier(&self) -> watch::Receiver<()>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc,
        Mutex,
    };

    #[test]
    fn feedback_handle_reports_once_with_the_given_values() {
        let sink = Arc::new(Mutex::new(Vec::new()));
        let handle = {
            let sink = sink.clone();
            BatchFeedbackHandle::new(move |report| sink.lock().unwrap().push(report))
        };

        handle.report(BatchExecutionReport {
            execution_time: 100,
            overhead_time: 7,
            completed: true,
        });

        let reported = sink.lock().unwrap().clone();
        assert_eq!(
            reported,
            vec![BatchExecutionReport {
                execution_time: 100,
                overhead_time: 7,
                completed: true,
            }]
        );
    }

    #[test]
    fn dropping_a_handle_without_reporting_is_silent() {
        let sink = Arc::new(Mutex::new(Vec::new()));
        let handle = {
            let sink = sink.clone();
            BatchFeedbackHandle::new(move |report| sink.lock().unwrap().push(report))
        };

        // Drop without reporting — the producer tolerates missing feedback, so
        // the closure must never run.
        drop(handle);

        assert!(
            sink.lock().unwrap().is_empty(),
            "dropping a handle must not fire the reporting closure"
        );
    }
}
