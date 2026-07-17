use crate::ports::{
    BatchExecutionReport,
    BatchFeedbackHandle,
    Filter,
    TransactionFiltered,
    TransactionSourceExecutableTransactions,
    TransactionsSource,
};
use fuel_core_executor::ports::{
    MaybeCheckedTransaction,
    PreconfirmationSenderPort,
    RelayerPort,
};
use fuel_core_types::{
    blockchain::primitives::DaBlockHeight,
    fuel_tx::{
        ConsensusParameters,
        Transaction,
    },
    fuel_vm::checked_transaction::IntoChecked,
    services::preconfirmation::Preconfirmation,
};
use std::{
    collections::VecDeque,
    sync::{
        Arc,
        Mutex,
        atomic::{
            AtomicUsize,
            Ordering,
        },
    },
};
use tokio::sync::watch;

/// Test-only observation point for the batch-feedback loop. The mock attaches a
/// real [`BatchFeedbackHandle`] to every non-empty batch it hands out (mirroring
/// what the txpool lane scheduler does when enabled). `created` counts those
/// dispatched batches; `reports` collects every [`BatchExecutionReport`] the
/// scheduler forwards back. With no blob-only responses, `created ==
/// reports.len()` is the leak-free / no-dropped-handle invariant, and each
/// report's `completed` flag reveals which completion path handled the batch.
#[derive(Clone, Default)]
pub struct FeedbackSink {
    created: Arc<AtomicUsize>,
    reports: Arc<Mutex<Vec<BatchExecutionReport>>>,
}

impl FeedbackSink {
    /// Number of non-empty batches handed out (each got a feedback handle).
    pub fn created(&self) -> usize {
        self.created.load(Ordering::Relaxed)
    }

    /// All reports forwarded back by the scheduler, in arrival order.
    pub fn reports(&self) -> Vec<BatchExecutionReport> {
        self.reports.lock().expect("Mutex poisoned").clone()
    }
}

#[derive(Debug, Clone)]
pub struct MockRelayer;
impl RelayerPort for MockRelayer {
    fn enabled(&self) -> bool {
        true
    }
    fn get_events(
        &self,
        _da_height: &DaBlockHeight,
    ) -> anyhow::Result<Vec<fuel_core_types::services::relayer::Event>> {
        Ok(vec![])
    }
}
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct PoolRequestParams {
    pub gas_limit: u64,
    pub tx_count_limit: u16,
    pub block_transaction_size_limit: u64,
    pub filter: Filter,
}
pub struct MockTransactionsSource {
    response_queue: Arc<Mutex<VecDeque<MockTxPoolResponse>>>,
    /// When set, a real feedback handle is attached to every non-empty batch and
    /// its completion report is collected here (see [`FeedbackSink`]).
    feedback: Option<FeedbackSink>,
}
#[derive(Debug)]
pub struct MockTxPoolResponse {
    pub transactions: Vec<MaybeCheckedTransaction>,
    pub filtered: TransactionFiltered,
    pub filter: Option<Filter>,
    pub gas_limit_lt: Option<u64>,
    pub selection_worker_count: Option<usize>,
}
impl MockTxPoolResponse {
    pub fn new(transactions: &[&Transaction], filtered: TransactionFiltered) -> Self {
        Self {
            transactions: into_checked_txs(transactions),
            filtered,
            filter: None,
            gas_limit_lt: None,
            selection_worker_count: None,
        }
    }
    pub fn assert_filter(self, filter: Filter) -> Self {
        Self {
            transactions: self.transactions,
            filtered: self.filtered,
            filter: Some(filter),
            gas_limit_lt: self.gas_limit_lt,
            selection_worker_count: self.selection_worker_count,
        }
    }
    pub fn assert_gas_limit_lt(self, gas_limit: u64) -> Self {
        Self {
            transactions: self.transactions,
            filtered: self.filtered,
            filter: self.filter,
            gas_limit_lt: Some(gas_limit),
            selection_worker_count: self.selection_worker_count,
        }
    }
    pub fn assert_selection_worker_count(self, selection_worker_count: usize) -> Self {
        Self {
            transactions: self.transactions,
            filtered: self.filtered,
            filter: self.filter,
            gas_limit_lt: self.gas_limit_lt,
            selection_worker_count: Some(selection_worker_count),
        }
    }
}
pub struct MockTxPool {
    response_queue: Arc<Mutex<VecDeque<MockTxPoolResponse>>>,
}
impl MockTxPool {
    pub fn push_response(&self, response: MockTxPoolResponse) {
        let response_queue = self.response_queue.clone();
        let mut response_queue = response_queue.lock().expect("Mutex poisoned");
        response_queue.push_back(response);
    }
}
impl MockTransactionsSource {
    pub fn new() -> (Self, MockTxPool) {
        let response_queue = Arc::new(Mutex::new(VecDeque::new()));
        (
            Self {
                response_queue: response_queue.clone(),
                feedback: None,
            },
            MockTxPool { response_queue },
        )
    }

    /// Like [`Self::new`] but the source attaches a batch-feedback handle to
    /// every non-empty batch it hands out and collects the reports into the
    /// returned [`FeedbackSink`], so tests can observe the executor's feedback
    /// loop on all completion paths.
    pub fn new_with_feedback() -> (Self, MockTxPool, FeedbackSink) {
        let response_queue = Arc::new(Mutex::new(VecDeque::new()));
        let sink = FeedbackSink::default();
        (
            Self {
                response_queue: response_queue.clone(),
                feedback: Some(sink.clone()),
            },
            MockTxPool { response_queue },
            sink,
        )
    }
}
impl TransactionsSource for MockTransactionsSource {
    async fn get_executable_transactions(
        &self,
        gas_limit: u64,
        tx_count_limit: u32,
        _block_transaction_size_limit: u64,
        selection_worker_count: usize,
        filter: Filter,
    ) -> anyhow::Result<TransactionSourceExecutableTransactions> {
        let mut response_queue = self.response_queue.lock().expect("Mutex poisoned");
        if let Some(response) = response_queue.pop_front() {
            assert!(response.transactions.len() <= tx_count_limit as usize);
            if let Some(expected_filter) = &response.filter {
                assert_eq!(expected_filter, &filter);
            }
            if let Some(expected_gas_limit) = &response.gas_limit_lt {
                assert!(
                    expected_gas_limit >= &gas_limit,
                    "Expected gas limit to be less than or equal to {}, but got {}",
                    expected_gas_limit,
                    gas_limit,
                );
            }
            if let Some(expected_selection_worker_count) = response.selection_worker_count
            {
                assert_eq!(expected_selection_worker_count, selection_worker_count);
            }
            // Attach a real feedback handle to non-empty batches when the sink is
            // enabled (empty responses never become a dispatched batch, so they
            // get no handle — keeping `created` == dispatched-batch count).
            let feedback_handle = match (&self.feedback, response.transactions.is_empty())
            {
                (Some(sink), false) => {
                    sink.created.fetch_add(1, Ordering::Relaxed);
                    let reports = sink.reports.clone();
                    Some(BatchFeedbackHandle::new(move |report| {
                        reports.lock().expect("Mutex poisoned").push(report);
                    }))
                }
                _ => None,
            };
            Ok(TransactionSourceExecutableTransactions {
                transactions: response.transactions,
                anchor_contract_ids: vec![],
                filtered: response.filtered,
                filter: response.filter.unwrap_or(filter),
                feedback_handle,
            })
        } else {
            Ok(TransactionSourceExecutableTransactions {
                transactions: vec![],
                anchor_contract_ids: vec![],
                filtered: TransactionFiltered::NotFiltered,
                filter,
                feedback_handle: None,
            })
        }
    }

    fn get_new_transactions_notifier(&self) -> watch::Receiver<()> {
        // This is a mock implementation, so we return a dummy Notify instance
        let (_, rx) = watch::channel(());
        rx
    }
}
fn into_checked_txs(txs: &[&Transaction]) -> Vec<MaybeCheckedTransaction> {
    txs.iter()
        .map(|&tx| {
            tx.clone()
                .into_checked_basic(0u32.into(), &ConsensusParameters::default())
                .unwrap()
                .into()
        })
        .map(|tx| MaybeCheckedTransaction::CheckedTransaction(tx, 0))
        .collect()
}
#[derive(Clone, Debug)]
pub struct MockPreconfirmationSender;

impl PreconfirmationSenderPort for MockPreconfirmationSender {
    fn send(
        &self,
        _preconfirmations: Vec<Preconfirmation>,
    ) -> impl Future<Output = ()> + Send {
        futures::future::ready(())
    }
    fn try_send(&self, preconfirmations: Vec<Preconfirmation>) -> Vec<Preconfirmation> {
        preconfirmations
    }
}
