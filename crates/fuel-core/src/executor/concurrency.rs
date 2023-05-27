use crate::schema::{
    scalars::UtxoId,
    tx::types::Transaction,
};
use fuel_core_services::stream::{
    BoxStream,
    IntoBoxStream,
};
use fuel_core_types::fuel_tx::{
    ContractId,
    MessageId,
    Transaction,
    UtxoId,
};
use std::{
    collections::HashSet,
    mem,
    sync::{
        atomic::AtomicU64,
        Arc,
        Mutex,
    },
};
use tokio::{
    sync::mpsc,
    task::JoinHandle,
};
use tokio_stream::{
    wrappers::ReceiverStream,
    StreamExt,
};
use tracing::error;

/// Demuxes a linear stream of transactions into independent streams based on the desired
/// concurrency level, taking into account utxo dependencies.
/// This is a stateful object that can be reused between blocks. Each demuxed stream will only yield
/// the configured gas limit per stream, however it will continue to queue incoming transactions.
/// When the stream has the gas counters reset (between each block), the streams will resume
/// yielding transactions.
/// If a stream has reached its buffer capacity, it will apply backpressure on the entire demuxer.
/// This could be improved in the future with a dead-letter queue pattern or similar mechanism
/// that allows transactions to be skipped over and retried later when there is more capacity.
pub struct DemuxedConcurrentTransactionStream {
    /// The source input stream driving the demuxer
    input_stream: Option<BoxStream<Transaction>>,
    /// configures the maximum amount of gas allowed to be yielded by each stream
    per_stream_gas_limit: u64,
    /// track the current amount of gas consumed by each stream
    gas_counters: Arc<Vec<AtomicU64>>,
    /// The number of concurrency groups (i.e. cpus) that can be parallelized across.
    concurrency_groups: usize,
    /// A distinct stream of transactions for each concurrency group
    streams: Vec<BoxStream<Transaction>>,
    stream_senders: Arc<Vec<mpsc::Sender<Transaction>>>,
    /// track all utxo ids associated with a stream
    utxo_ids: Arc<Vec<Mutex<HashSet<UtxoId>>>>,
    /// track all contract ids associated with a stream
    contract_ids: Arc<Vec<Mutex<HashSet<ContractId>>>>,
    /// track all message ids associated with a stream
    message_ids: Arc<Vec<Mutex<HashSet<MessageId>>>>,
    /// the handle to the background task that ingests transactions into the demuxer
    task_handle: Option<JoinHandle<()>>,
}

impl DemuxedConcurrentTransactionStream {
    pub fn new(
        concurrency_level: usize,
        input_stream: BoxStream<Transaction>,
        per_stream_gas_limit: u64,
    ) -> Self {
        let gas_counters =
            Arc::new((0..concurrency_level).map(|_| AtomicU64::new(0)).collect());
        let channels = (0..concurrency_level)
            .map(|_| mpsc::channel(1))
            .collect::<Vec<_>>();
        let (stream_senders, streams) = channels.into_iter().fold(
            (vec![], vec![]),
            |(mut senders, mut receivers), (sender, receiver)| {
                senders.push(sender);
                receivers.push(ReceiverStream::new(receiver).into_boxed());
                (senders, receivers)
            },
        );

        let utxo_ids = Arc::new(
            (0..concurrency_level)
                .map(|_| Mutex::new(HashSet::new()))
                .collect(),
        );
        let contract_ids = Arc::new(
            (0..concurrency_level)
                .map(|_| Mutex::new(HashSet::new()))
                .collect(),
        );
        let message_ids = Arc::new(
            (0..concurrency_level)
                .map(|_| Mutex::new(HashSet::new()))
                .collect(),
        );

        Self {
            input_stream: Some(input_stream),
            per_stream_gas_limit,
            gas_counters,
            concurrency_groups: concurrency_level,
            streams,
            stream_senders: Arc::new(stream_senders),
            utxo_ids,
            contract_ids,
            message_ids,
            task_handle: None,
        }
    }

    /// starts the demuxer, which will populate each stream with transactions as a background task
    pub fn start(&mut self) {
        // do nothing if already started
        if self.task_handle.is_none() {
            return
        }

        let input_stream = mem::take(&mut self.input_stream);
        let gas_counters = self.gas_counters.clone();
        let stream_senders = self.stream_senders.clone();

        let handle = tokio::spawn(async move {
            if let Some(mut input_stream) = input_stream {
                for tx in input_stream.next().await {
                    // Check for any matching utxo ids, contract ids or message ids and add to the
                    // same stream. Otherwise, add to stream with the lowest gas counter.
                    // if the transaction has dependencies split across different streams,
                    // buffer it and continuously recheck for dependency conflicts until
                }
            } else {
                error!(
                    "Failed to start transaction stream demuxer, input stream is empty"
                );
            }
        });
        self.task_handle = Some(handle);
    }

    /// Reset the gas counters for all streams. This should be done to resume progress
    /// between each block.
    pub fn reset_gas_counters(&self) {}
}

fn find_matching_coin_id(
    tx: &Transaction,
    utxo_sets: Arc<Vec<HashSet<UtxoId>>>,
) -> usize {
    0
}

fn find_matching_contract_id(
    tx: &Transaction,
    contract_ids: Arc<Vec<HashSet<ContractId>>>,
) -> usize {
    0
}
