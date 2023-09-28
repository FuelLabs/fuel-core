//! # Importer Task
//! This module contains the import task which is responsible for
//! importing blocks from the network into the local blockchain.

use anyhow::anyhow;
use fuel_core_services::{
    SharedMutex,
    StateWatcher,
};
use fuel_core_types::{
    self,
    blockchain::{
        block::Block,
        SealedBlock,
        SealedBlockHeader,
    },
    fuel_types::BlockHeight,
    services::p2p::{
        PeerId,
        SourcePeer,
        Transactions,
    },
};
use futures::{
    stream::StreamExt,
    FutureExt,
    Stream,
    TryStreamExt,
};
use std::{
    future::Future,
    ops::{
        Range,
        RangeInclusive,
    },
    sync::Arc,
};
use tokio::{
    sync::Notify,
    task::JoinError,
};
use tracing::Instrument;

use crate::{
    ports::{
        BlockImporterPort,
        ConsensusPort,
        PeerReportReason,
        PeerToPeerPort,
    },
    state::State,
    tracing_helpers::TraceErr,
};

#[cfg(any(test, feature = "benchmarking"))]
/// Accessories for testing the sync. Available only when compiling under test
/// or benchmarking.
pub mod test_helpers;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod back_pressure_tests;

#[derive(Clone, Copy, Debug)]
/// Parameters for the import task.
pub struct Config {
    /// The maximum number of get transaction requests to make in a single batch.
    pub block_stream_buffer_size: usize,
    /// The maximum number of headers to request in a single batch.
    pub header_batch_size: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            block_stream_buffer_size: 10,
            header_batch_size: 100,
        }
    }
}

/// The combination of shared state, configuration, and services that define
/// import behavior.
pub struct Import<P, E, C> {
    /// Shared state between import and sync tasks.
    state: SharedMutex<State>,
    /// Notify import when sync has new work.
    notify: Arc<Notify>,
    /// Configuration parameters.
    params: Config,
    /// Network port.
    p2p: Arc<P>,
    /// Executor port.
    executor: Arc<E>,
    /// Consensus port.
    consensus: Arc<C>,
}

impl<P, E, C> Import<P, E, C> {
    /// Configure an import behavior from a shared state, configuration and
    /// services that can be executed by an ImportTask.
    pub fn new(
        state: SharedMutex<State>,
        notify: Arc<Notify>,
        params: Config,
        p2p: Arc<P>,
        executor: Arc<E>,
        consensus: Arc<C>,
    ) -> Self {
        Self {
            state,
            notify,
            params,
            p2p,
            executor,
            consensus,
        }
    }

    /// Signal other asynchronous tasks that an import event has occurred.
    pub fn notify_one(&self) {
        self.notify.notify_one()
    }
}

#[derive(Debug)]
struct Batch<T> {
    peer: PeerId,
    range: Range<u32>,
    results: Vec<T>,
}

impl<T> Batch<T> {
    pub fn empty(peer: PeerId, range: Range<u32>) -> Self {
        Self {
            peer,
            range,
            results: vec![],
        }
    }

    pub fn new(peer: PeerId, range: Range<u32>, results: Vec<T>) -> Self {
        Self {
            peer,
            range,
            results,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.results.is_empty()
    }

    pub fn is_err(&self) -> bool {
        self.results.len() < self.range.len()
    }
}

type SealedHeaderBatch = Batch<SealedBlockHeader>;
type SealedBlockBatch = Batch<SealedBlock>;

impl SealedBlockBatch {
    fn err(&self) -> Option<ImportError> {
        self.is_err().then(|| ImportError::MissingTransactions)
    }
}

#[derive(Debug, derive_more::Display)]
enum ImportError {
    ConsensusError(anyhow::Error),
    ExecutionError(anyhow::Error),
    NoSuitablePeer,
    MissingBlockHeaders,
    MissingTransactions,
    BadBlockHeader,
    JoinError(JoinError),
    Other(anyhow::Error),
}

impl From<anyhow::Error> for ImportError {
    fn from(value: anyhow::Error) -> Self {
        ImportError::Other(value)
    }
}

impl<P, E, C> Import<P, E, C>
where
    P: PeerToPeerPort + Send + Sync + 'static,
    E: BlockImporterPort + Send + Sync + 'static,
    C: ConsensusPort + Send + Sync + 'static,
{
    #[tracing::instrument(skip_all)]
    /// Execute imports until a shutdown is requested.
    pub async fn import(&self, shutdown: &mut StateWatcher) -> anyhow::Result<bool> {
        self.import_inner(shutdown).await.map_err(|e| anyhow!(e))?;

        Ok(wait_for_notify_or_shutdown(&self.notify, shutdown).await)
    }

    async fn import_inner(&self, shutdown: &StateWatcher) -> Result<(), ImportError> {
        // If there is a range to process, launch the stream.
        if let Some(range) = self.state.apply(|s| s.process_range()) {
            // Launch the stream to import the range.
            let count = self.launch_stream(range.clone(), shutdown).await;

            // Get the size of the range.
            let range_len = range.size_hint().0 as u32;

            // If we did not process the entire range, mark the failed heights as failed.
            if (count as u32) < range_len {
                let incomplete_range = (*range.start() + count as u32)..=*range.end();
                self.state
                    .apply(|s| s.failed_to_process(incomplete_range.clone()));
                Err(anyhow::anyhow!(
                    "Failed to import range of blocks: {:?}",
                    incomplete_range
                ))?;
            }
        }
        Ok(())
    }

    #[tracing::instrument(skip(self, shutdown))]
    /// Launches a stream to import and execute a range of blocks.
    ///
    /// This stream will process all blocks up to the given range or
    /// an error occurs.
    /// If an error occurs, the preceding blocks still be processed
    /// and the error will be returned.
    async fn launch_stream(
        &self,
        range: RangeInclusive<u32>,
        shutdown: &StateWatcher,
    ) -> usize {
        let Self {
            state,
            params,
            p2p,
            executor,
            consensus,
            ..
        } = &self;

        let shutdown_signal = shutdown.clone();
        let (shutdown_guard, mut shutdown_guard_recv) =
            tokio::sync::mpsc::channel::<()>(1);

        let block_height = BlockHeight::from(*range.end());
        let peer = select_peer(block_height, p2p.as_ref()).await;
        if peer.is_err() {
            return 0
        }
        let peer = peer.expect("Checked");

        let block_stream = get_block_stream(
            peer.clone(),
            range.clone(),
            params,
            p2p.clone(),
            consensus.clone(),
        );
        let range = *range.start()..(*range.end() + 1);
        let result = block_stream
            .map({
                let peer = peer.clone();
                let range = range.clone();
                move |stream_block_batch| {
                    let shutdown_guard = shutdown_guard.clone();
                    let shutdown_signal = shutdown_signal.clone();
                    let peer = peer.clone();
                    let range = range.clone();
                    tokio::spawn(async move {
                        // Hold a shutdown sender for the lifetime of the spawned task
                        let _shutdown_guard = shutdown_guard.clone();
                        let mut shutdown_signal = shutdown_signal.clone();
                        let peer = peer.clone();
                        let range = range.clone();
                        tokio::select! {
                        // Stream a batch of blocks
                        blocks = stream_block_batch => blocks,
                        // If a shutdown signal is received during the stream, terminate early and
                        // return an empty response
                        _ = shutdown_signal.while_started() => Batch::empty(peer, range)
                    }
                    }).map(|task| task.map_err(ImportError::JoinError))
                }
            })
            // Request up to `block_stream_buffer_size` transactions from the network.
            .buffered(params.block_stream_buffer_size)
            // Continue the stream until the shutdown signal is received.
            .take_until({
                let mut s = shutdown.clone();
                async move {
                    let _ = s.while_started().await;
                    tracing::info!("In progress import stream shutting down");
                }
            })
            .map({
                let peer = peer.clone();
                let range = range.clone();
                move |result| result.unwrap_or({
                    let peer = peer.clone();
                    let range = range.clone();
                    SealedBlockBatch::empty(peer, range)
                })
            })
            .then({
                let peer = peer.clone();
                let range = range.clone();
                    move |batch| {
                        let peer = peer.clone();
                        let range = range.clone();
                        async move {
                            let err = batch.err();
                            let sealed_blocks = futures::stream::iter(batch.results);
                            let res = sealed_blocks.then(|sealed_block| async {
                                execute_and_commit(executor.as_ref(), state, sealed_block).await
                            }).try_collect::<Vec<_>>().await.and_then(|v| err.map_or(Ok(v), Err));
                            match res {
                                Ok(v) => {
                                    report_peer(p2p.as_ref(), peer.clone(), PeerReportReason::SuccessfulBlockImport);
                                    Batch::new(peer, range, v)
                                },
                                Err(e) => {
                                    // If this fails, then it means that consensus has approved a block that is invalid.
                                    // This would suggest a more serious issue than a bad peer, e.g. a fork or an out-of-date client.
                                    tracing::error!("Failed to execute and commit block from peer {:?}: {:?}", peer, e);
                                    Batch::empty(peer, range)

                                },
                            }
                        }
                        .instrument(tracing::debug_span!("execute_and_commit"))
                        .in_current_span()
                    }
                }
            )

            // Continue the stream unless an error occurs.
            .into_scan_err()
            .scan_err()
            // Count the number of successfully executed blocks and
            // find any errors.
            // Fold the stream into a count and any errors.
            .fold(0usize, |count, batch| async move {
                count + batch.results.len()
            })
            // .in_current_span()
            .await;

        // Wait for any spawned tasks to shutdown
        let _ = shutdown_guard_recv.recv().await;
        result
    }
}

fn get_block_stream<
    P: PeerToPeerPort + Send + Sync + 'static,
    C: ConsensusPort + Send + Sync + 'static,
>(
    peer_id: PeerId,
    range: RangeInclusive<u32>,
    params: &Config,
    p2p: Arc<P>,
    consensus: Arc<C>,
) -> impl Stream<Item = impl Future<Output = SealedBlockBatch>> + '_ {
    let header_stream =
        get_header_batch_stream(peer_id.clone(), range.clone(), params, p2p.clone());
    let range = *range.start()..(*range.end() + 1);
    header_stream
        .map({
            let peer_id = peer_id.clone();
            let consensus = consensus.clone();
            let p2p = p2p.clone();
            move |header_batch| {
                header_batch
                    .results
                    .into_iter()
                    .map({
                        let peer_id = peer_id.clone();
                        let consensus = consensus.clone();
                        let p2p = p2p.clone();
                        move |header| {
                            check_sealed_header(
                                &header,
                                peer_id.clone(),
                                p2p.clone(),
                                consensus.clone(),
                            )?;
                            Ok(header)
                        }
                    })
                    .take_while(|result| result.is_ok())
                    .filter_map(|result: Result<SealedBlockHeader, ImportError>| {
                        result.ok()
                    })
                    .collect::<Vec<_>>()
            }
        })
        .map({
            let consensus = consensus.clone();
            move |valid_headers| {
                let consensus = consensus.clone();
                async move {
                    if let Some(header) = valid_headers.last() {
                        await_da_height(header, consensus.as_ref()).await?
                    };
                    Result::<_, ImportError>::Ok(valid_headers)
                }
            }
        })
        .map({
            let peer_id = peer_id.clone();
            let range = range.clone();
            let p2p = p2p.clone();
            move |headers| {
                let peer_id = peer_id.clone();
                let range = range.clone();
                let p2p = p2p.clone();
                async move {
                    let headers = headers.await.unwrap_or(Default::default());
                    let headers =
                        SealedHeaderBatch::new(peer_id.clone(), range.clone(), headers);
                    get_blocks(p2p, headers).await
                }
            }
        })
}

fn get_header_batch_stream<'a, P: PeerToPeerPort + Send + Sync + 'static>(
    peer: PeerId,
    range: RangeInclusive<u32>,
    params: &'a Config,
    p2p: Arc<P>,
) -> impl Stream<Item = SealedHeaderBatch> + 'a {
    let Config {
        header_batch_size, ..
    } = params;
    let ranges = range_chunks(range, *header_batch_size);
    futures::stream::iter(ranges).then(move |range| {
        let peer = peer.clone();
        let p2p = p2p.clone();
        async move { get_headers_batch(peer, range, p2p).await }
    })
}

fn range_chunks(
    range: RangeInclusive<u32>,
    chunk_size: u32,
) -> impl Iterator<Item = RangeInclusive<u32>> {
    let end = *range.end();
    range.step_by(chunk_size as usize).map(move |chunk_start| {
        let block_end = (chunk_start + chunk_size).min(end);
        chunk_start..=block_end
    })
}

fn check_sealed_header<
    P: PeerToPeerPort + Send + Sync + 'static,
    C: ConsensusPort + Send + Sync + 'static,
>(
    header: &SealedBlockHeader,
    peer_id: PeerId,
    p2p: Arc<P>,
    consensus: Arc<C>,
) -> Result<(), ImportError> {
    let validity = consensus
        .check_sealed_header(header)
        .map_err(ImportError::ConsensusError)
        .trace_err("Failed to check consensus on header")?;
    if validity {
        Ok(())
    } else {
        report_peer(
            p2p.as_ref(),
            peer_id.clone(),
            PeerReportReason::BadBlockHeader,
        );
        Err(ImportError::BadBlockHeader)
    }
}

async fn await_da_height<C: ConsensusPort + Send + Sync + 'static>(
    header: &SealedBlockHeader,
    consensus: &C,
) -> Result<(), ImportError> {
    consensus
        .await_da_height(&header.entity.da_height)
        .await
        .map_err(ImportError::ConsensusError)?;
    Ok(())
}

/// Waits for a notify or shutdown signal.
/// Returns true if the notify signal was received.
async fn wait_for_notify_or_shutdown(
    notify: &Notify,
    shutdown: &mut StateWatcher,
) -> bool {
    let n = notify.notified();
    let s = shutdown.while_started();
    futures::pin_mut!(n);
    futures::pin_mut!(s);

    // Select the first signal to be received.
    let r = futures::future::select(n, s).await;

    // Check if the notify signal was received.
    matches!(r, futures::future::Either::Left(_))
}

async fn select_peer<P>(block_height: BlockHeight, p2p: &P) -> Result<PeerId, ImportError>
where
    P: PeerToPeerPort + Send + Sync + 'static,
{
    tracing::debug!("getting peer for block height {}", block_height);
    let res = p2p.select_peer(block_height).await;
    match res {
        Ok(Some(peer_id)) => Ok(peer_id),
        Ok(None) => Err(ImportError::NoSuitablePeer),
        Err(e) => Err(e.into()),
    }
}

async fn get_sealed_block_headers<P>(
    peer: PeerId,
    range: Range<u32>,
    p2p: &P,
) -> Result<Vec<SealedBlockHeader>, ImportError>
where
    P: PeerToPeerPort + Send + Sync + 'static,
{
    tracing::debug!(
        "getting header range from {} to {} inclusive",
        range.start,
        range.end
    );
    let range = peer.clone().bind(range);
    let res = p2p.get_sealed_block_headers(range).await;
    let SourcePeer { data: headers, .. } = res;
    match headers {
        Ok(Some(headers)) => Ok(headers),
        Ok(None) => {
            report_peer(p2p, peer.clone(), PeerReportReason::MissingBlockHeaders);
            Err(ImportError::MissingBlockHeaders)
        }
        Err(e) => Err(e.into()),
    }
}

async fn get_transactions<P>(
    peer_id: PeerId,
    range: Range<u32>,
    p2p: &P,
) -> Result<Vec<Transactions>, ImportError>
where
    P: PeerToPeerPort + Send + Sync + 'static,
{
    let range = peer_id.clone().bind(range);
    let res = p2p.get_transactions_2(range).await;
    match res {
        Ok(Some(transactions)) => Ok(transactions),
        Ok(None) => {
            report_peer(p2p, peer_id.clone(), PeerReportReason::MissingTransactions);
            Err(ImportError::MissingTransactions)
        }
        Err(e) => Err(e.into()),
    }
}

async fn get_headers_batch<P>(
    peer_id: PeerId,
    range: RangeInclusive<u32>,
    p2p: Arc<P>,
) -> SealedHeaderBatch
where
    P: PeerToPeerPort + Send + Sync + 'static,
{
    tracing::debug!(
        "getting header range from {} to {} inclusive",
        range.start(),
        range.end()
    );
    let start = *range.start();
    let end = *range.end() + 1;
    let range = start..end;
    let res =
        get_sealed_block_headers(peer_id.clone(), range.clone(), p2p.as_ref()).await;
    match res {
        Ok(headers) => {
            let headers = headers.into_iter();
            let heights = range.clone().map(BlockHeight::from);
            let headers = headers
                .zip(heights)
                .take_while(move |(header, expected_height)| {
                    let height = header.entity.height();
                    height == expected_height
                })
                .map(|(header, _)| header)
                .collect::<Vec<_>>();
            if let Some(expected_len) = end.checked_sub(start) {
                if headers.len() != expected_len as usize {
                    report_peer(
                        p2p.as_ref(),
                        peer_id.clone(),
                        PeerReportReason::MissingBlockHeaders,
                    );
                }
            }
            Batch::new(peer_id, range.clone(), headers)
        }
        Err(_e) => Batch::empty(peer_id, range.clone()),
    }
}

fn report_peer<P>(p2p: &P, peer_id: PeerId, reason: PeerReportReason)
where
    P: PeerToPeerPort + Send + Sync + 'static,
{
    tracing::info!("Reporting peer for {:?}", reason);

    // Failure to report a peer is a non-fatal error; ignore the error
    let _ = p2p
        .report_peer(peer_id.clone(), reason)
        .trace_err(&format!("Failed to report peer {:?}", peer_id));
}

/// Get blocks correlating to the headers from a specific peer
#[tracing::instrument(skip(p2p, headers))]
async fn get_blocks<P>(p2p: Arc<P>, headers: SealedHeaderBatch) -> SealedBlockBatch
where
    P: PeerToPeerPort + Send + Sync + 'static,
{
    if headers.is_empty() {
        return SealedBlockBatch::empty(headers.peer, headers.range)
    }

    let Batch {
        results: headers,
        peer,
        range,
        ..
    } = headers;
    let maybe_txs = get_transactions(peer.clone(), range.clone(), p2p.as_ref()).await;
    match maybe_txs {
        Ok(transaction_data) => {
            let iter = headers.into_iter().zip(transaction_data);
            let mut blocks = vec![];
            for (block_header, transactions) in iter {
                let block_header = block_header.clone();
                let SealedBlockHeader {
                    consensus,
                    entity: header,
                } = block_header;
                let block =
                    Block::try_from_executed(header, transactions.0).map(|block| {
                        SealedBlock {
                            entity: block,
                            consensus,
                        }
                    });
                if let Some(block) = block {
                    blocks.push(block);
                } else {
                    report_peer(
                        p2p.as_ref(),
                        peer.clone(),
                        PeerReportReason::InvalidTransactions,
                    );
                }
            }
            Batch::new(peer, range, blocks)
        }
        Err(_error) => Batch::empty(peer, range),
    }
}

#[tracing::instrument(
    skip_all,
    fields(
        height = **block.entity.header().height(),
        id = %block.entity.header().consensus.generated.application_hash
    ),
    err
)]
async fn execute_and_commit<E>(
    executor: &E,
    state: &SharedMutex<State>,
    block: SealedBlock,
) -> Result<(), ImportError>
where
    E: BlockImporterPort + Send + Sync + 'static,
{
    // Execute and commit the block.
    let height = *block.entity.header().height();
    let r = executor
        .execute_and_commit(block)
        .await
        .map_err(ImportError::ExecutionError);

    // If the block executed successfully, mark it as committed.
    if r.is_ok() {
        state.apply(|s| s.commit(*height));
    } else {
        tracing::error!("Execution of height {} failed: {:?}", *height, r);
    }
    r
}

/// Extra stream utilities.
trait StreamUtil: Sized {
    fn into_scan_err(self) -> ScanErr<Self> {
        ScanErr(self)
    }
}

impl<S> StreamUtil for S {}

struct ScanErr<S>(S);

impl<S> ScanErr<S> {
    /// Scan the stream for errors.
    fn scan_err<'a, T: 'a>(self) -> impl Stream<Item = Batch<T>> + 'a
    where
        S: Stream<Item = Batch<T>> + Send + 'a,
    {
        let stream = self.0.boxed::<'a>();
        futures::stream::unfold((false, stream), |(mut err, mut stream)| async move {
            if err {
                None
            } else {
                let batch = stream.next().await?;
                err = batch.is_err();
                Some((batch, (err, stream)))
            }
        })
    }
}
