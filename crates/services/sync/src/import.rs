//! # Importer Task
//! This module contains the import task which is responsible for
//! importing blocks from the network into the local blockchain.

use cache::{
    Cache,
    CachedDataBatch,
};
use fuel_core_services::{
    SharedMutex,
    StateWatcher,
    TraceErr,
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
};
use std::{
    future::Future,
    num::NonZeroU32,
    ops::{
        Range,
        RangeInclusive,
    },
    sync::Arc,
};
use tokio::{
    pin,
    sync::{
        mpsc,
        Notify,
    },
    task::JoinHandle,
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
};

mod cache;

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
    pub header_batch_size: usize,
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
    /// A cache of already validated header or blocks.
    cache: Cache,
}

/// The data that is fetched either in the network or in the cache for a range of headers or blocks.
#[derive(Debug, Clone)]
enum BlockHeaderData {
    /// The headers (or full blocks) have been fetched and checked.
    Cached(CachedDataBatch),
    /// The headers has just been fetched from the network.
    Fetched(Batch<SealedBlockHeader>),
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
            cache: Cache::new(),
        }
    }

    /// Signal other asynchronous tasks that an import event has occurred.
    pub fn notify_one(&self) {
        self.notify.notify_one()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Batch<T> {
    peer: Option<PeerId>,
    range: Range<u32>,
    results: Vec<T>,
}

impl<T> Batch<T> {
    pub fn new(peer: Option<PeerId>, range: Range<u32>, results: Vec<T>) -> Self {
        Self {
            peer,
            range,
            results,
        }
    }

    pub fn is_err(&self) -> bool {
        self.results.len() < self.range.len()
    }
}

type SealedHeaderBatch = Batch<SealedBlockHeader>;
type SealedBlockBatch = Batch<SealedBlock>;

impl<P, E, C> Import<P, E, C>
where
    P: PeerToPeerPort + Send + Sync + 'static,
    E: BlockImporterPort + Send + Sync + 'static,
    C: ConsensusPort + Send + Sync + 'static,
{
    #[tracing::instrument(skip_all)]
    /// Execute imports until a shutdown is requested.
    pub async fn import(&mut self, shutdown: &mut StateWatcher) -> anyhow::Result<bool> {
        self.import_inner(shutdown).await?;

        Ok(wait_for_notify_or_shutdown(&self.notify, shutdown).await)
    }

    async fn import_inner(&mut self, shutdown: &StateWatcher) -> anyhow::Result<()> {
        // If there is a range to process, launch the stream.
        if let Some(range) = self.state.apply(|s| s.process_range()) {
            // Launch the stream to import the range.
            let count = self.launch_stream(range.clone(), shutdown).await;

            // Get the size of the range.
            let range_len = range.size_hint().0;

            // If we did not process the entire range, mark the failed heights as failed.
            if count < range_len {
                let count = u32::try_from(count)
                    .expect("Size of the range can't be more than maximum `BlockHeight`");
                let incomplete_range = range.start().saturating_add(count)..=*range.end();
                self.state
                    .apply(|s| s.failed_to_process(incomplete_range.clone()));
                return Err(anyhow::anyhow!(
                    "Failed to import range of blocks: {:?}",
                    incomplete_range
                ));
            }
        }
        Ok(())
    }

    fn fetch_batches_task(
        &self,
        range: RangeInclusive<u32>,
        shutdown: &StateWatcher,
    ) -> (JoinHandle<()>, mpsc::Receiver<SealedBlockBatch>) {
        let Self {
            params,
            p2p,
            consensus,
            cache,
            ..
        } = &self;
        let batch_size = u32::try_from(params.header_batch_size)
            .expect("Header batch size must be less u32::MAX");
        let batch_size =
            NonZeroU32::new(batch_size).expect("Header batch size must be non-zero");

        let (batch_sender, batch_receiver) = mpsc::channel(1);

        let fetch_batches_task = tokio::spawn({
            let params = *params;
            let p2p = p2p.clone();
            let consensus = consensus.clone();
            let cache = cache.clone();
            let block_stream_buffer_size = params.block_stream_buffer_size;
            let mut shutdown_signal = shutdown.clone();
            async move {
                let block_stream = get_block_stream(
                    range.clone(),
                    batch_size,
                    p2p,
                    consensus,
                    cache.clone(),
                );

                let shutdown_future = {
                    let mut s = shutdown_signal.clone();
                    async move {
                        let _ = s.while_started().await;
                        tracing::info!("In progress import stream shutting down");
                    }
                };

                let stream = block_stream
                    .map({
                        let shutdown_signal = shutdown_signal.clone();
                        move |stream_block_batch| {
                            let mut shutdown_signal = shutdown_signal.clone();
                            tokio::spawn(async move {
                                tokio::select! {
                                    biased;
                                    // If a shutdown signal is received during the stream, terminate early and
                                    // return an empty response
                                    _ = shutdown_signal.while_started() => None,
                                    // Stream a batch of blocks
                                    blocks = stream_block_batch => Some(blocks),
                                }
                            }).map(|task| {
                                task.trace_err("Failed to join the task").ok().flatten()
                            })
                        }
                    })
                    // Request up to `block_stream_buffer_size` headers/transactions from the network.
                    .buffered(block_stream_buffer_size)
                    // Continue the stream until the shutdown signal is received.
                    .take_until(shutdown_future)
                    .into_scan_none()
                    .scan_none()
                    .into_scan_err()
                    .scan_err();

                pin!(stream);

                while let Some(block_batch) = stream.next().await {
                    tokio::select! {
                        biased;
                        _ = shutdown_signal.while_started() => {
                            break;
                        },
                        result = batch_sender.send(block_batch) => {
                            if result.is_err() {
                                break
                            }
                        },
                    }
                }
            }
        });

        (fetch_batches_task, batch_receiver)
    }

    #[tracing::instrument(skip(self, shutdown))]
    /// Launches a stream to import and execute a range of blocks.
    ///
    /// This stream will process all blocks up to the given range or
    /// an error occurs.
    /// If an error occurs, the preceding blocks still be processed
    /// and the error will be returned.
    async fn launch_stream(
        &mut self,
        range: RangeInclusive<u32>,
        shutdown: &StateWatcher,
    ) -> usize {
        let Self {
            state,
            p2p,
            executor,
            ..
        } = &self;

        let (fetch_batches_task, batch_receiver) =
            self.fetch_batches_task(range, shutdown);
        let result = tokio_stream::wrappers::ReceiverStream::new(batch_receiver)
            .then(|batch| {
                let mut cache = self.cache.clone();
                async move {
                    let Batch {
                        peer,
                        range,
                        results,
                    } = batch;

                    let mut done = vec![];
                    let mut shutdown = shutdown.clone();
                    for sealed_block in results {
                        let height = *sealed_block.entity.header().height();
                        let res = tokio::select! {
                            biased;
                            _ = shutdown.while_started() => {
                                break;
                            },
                            res = execute_and_commit(executor.as_ref(), state, sealed_block) => {
                                cache.remove_element(&height);
                                res
                            },
                        };

                        match &res {
                            Ok(_) => {
                                done.push(());
                            },
                            Err(e) => {
                                // If this fails, then it means that consensus has approved a block that is invalid.
                                // This would suggest a more serious issue than a bad peer, e.g. a fork or an out-of-date client.
                                tracing::error!("Failed to execute and commit block from peer {:?}: {:?}", peer, e);
                                break;
                            },
                        };
                    }

                    let batch = Batch::new(peer.clone(), range.clone(), done);

                    if !batch.is_err() {
                        report_peer(p2p, peer, PeerReportReason::SuccessfulBlockImport);
                    }

                    batch
                }
                .instrument(tracing::debug_span!("execute_and_commit"))
                .in_current_span()
            })
            // Continue the stream unless an error occurs.
            .into_scan_err()
            .scan_err()
            // Count the number of successfully executed blocks.
            // Fold the stream into a count.
            .fold(0usize, |count, batch| async move {
                count.checked_add(batch.results.len()).expect("It is impossible to fetch so much data to overflow `usize`")
            })
            .await;

        // Wait for spawned task to finish
        let _ = fetch_batches_task
            .await
            .trace_err("Failed to join the fetch batches task");
        result
    }
}

fn get_block_stream<
    P: PeerToPeerPort + Send + Sync + 'static,
    C: ConsensusPort + Send + Sync + 'static,
>(
    range: RangeInclusive<u32>,
    header_batch_size: NonZeroU32,
    p2p: Arc<P>,
    consensus: Arc<C>,
    cache: Cache,
) -> impl Stream<Item = impl Future<Output = SealedBlockBatch>> {
    cache
        .get_chunks(range.clone(), header_batch_size)
        .map({
            let p2p = p2p.clone();
            move |cached_data_batch| {
                let p2p = p2p.clone();
                async move {
                    if let CachedDataBatch::None(range) = cached_data_batch {
                        BlockHeaderData::Fetched(get_headers_batch(range, &p2p).await)
                    } else {
                        BlockHeaderData::Cached(cached_data_batch)
                    }
                }
            }
        })
        .map({
            let p2p = p2p.clone();
            let consensus = consensus.clone();
            let cache = cache.clone();
            move |header_batch| {
                let p2p = p2p.clone();
                let consensus = consensus.clone();
                let mut cache = cache.clone();
                async move {
                    match header_batch.await {
                        BlockHeaderData::Cached(cached_data) => {
                            BlockHeaderData::Cached(cached_data)
                        }
                        BlockHeaderData::Fetched(fetched_batch) => {
                            let Batch {
                                peer,
                                range,
                                results,
                            } = fetched_batch;
                            let checked_headers = results
                                .into_iter()
                                .take_while(|header| {
                                    check_sealed_header(
                                        header,
                                        peer.clone(),
                                        &p2p,
                                        &consensus,
                                    )
                                })
                                .collect::<Vec<_>>();
                            let batch = Batch::new(peer, range.clone(), checked_headers);
                            if !batch.is_err() {
                                cache.insert_headers(batch.clone());
                            }
                            BlockHeaderData::Fetched(batch)
                        }
                    }
                }
            }
        })
        .map({
            let p2p = p2p.clone();
            let consensus = consensus.clone();
            move |headers| {
                let p2p = p2p.clone();
                let consensus = consensus.clone();
                let mut cache = cache.clone();
                async move {
                    match headers.await {
                        BlockHeaderData::Cached(CachedDataBatch::Blocks(batch)) => batch,
                        BlockHeaderData::Cached(CachedDataBatch::Headers(batch))
                        | BlockHeaderData::Fetched(batch) => {
                            let Batch {
                                peer,
                                range,
                                results,
                            } = batch;
                            if results.is_empty() {
                                SealedBlockBatch::new(peer, range, vec![])
                            } else {
                                await_da_height(
                                    results
                                        .last()
                                        .expect("We checked headers are not empty above"),
                                    &consensus,
                                )
                                .await;
                                let headers =
                                    SealedHeaderBatch::new(peer, range.clone(), results);
                                let batch = get_blocks(&p2p, headers).await;
                                if !batch.is_err() {
                                    cache.insert_blocks(batch.clone());
                                }
                                batch
                            }
                        }
                        BlockHeaderData::Cached(CachedDataBatch::None(_)) => {
                            tracing::error!("Cached data batch should never be created outside of the caching algorithm.");
                            Batch::new(None, 0..1, vec![])
                        }
                    }
                }
                .instrument(tracing::debug_span!("consensus_and_transactions"))
                .in_current_span()
            }
        })
}

fn check_sealed_header<
    P: PeerToPeerPort + Send + Sync + 'static,
    C: ConsensusPort + Send + Sync + 'static,
>(
    header: &SealedBlockHeader,
    peer_id: Option<PeerId>,
    p2p: &Arc<P>,
    consensus: &Arc<C>,
) -> bool {
    let validity = consensus
        .check_sealed_header(header)
        .trace_err("Failed to check consensus on header")
        .unwrap_or(false);
    if !validity {
        report_peer(p2p, peer_id.clone(), PeerReportReason::BadBlockHeader);
    }
    validity
}

async fn await_da_height<C: ConsensusPort + Send + Sync + 'static>(
    header: &SealedBlockHeader,
    consensus: &Arc<C>,
) {
    let _ = consensus
        .await_da_height(&header.entity.da_height)
        .await
        .trace_err("Failed to wait for DA layer to sync");
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

async fn get_sealed_block_headers<P>(
    range: Range<u32>,
    p2p: &Arc<P>,
) -> Option<SourcePeer<Vec<SealedBlockHeader>>>
where
    P: PeerToPeerPort + Send + Sync + 'static,
{
    tracing::debug!(
        "getting header range from {} to {} inclusive",
        range.start,
        range.end
    );
    p2p.get_sealed_block_headers(range)
        .await
        .trace_err("Failed to get headers")
        .ok()
        .map(|res| res.map(|data| data.unwrap_or_default()))
}

async fn get_transactions<P>(
    range: Range<u32>,
    peer_id: Option<PeerId>,
    p2p: &Arc<P>,
) -> Option<SourcePeer<Vec<Transactions>>>
where
    P: PeerToPeerPort + Send + Sync + 'static,
{
    match peer_id {
        Some(peer_id) => {
            let source_peer = peer_id.clone().bind(range.clone());
            let Ok(Some(txs)) = p2p
                .get_transactions_from_peer(source_peer)
                .await
                .trace_err("Failed to get transactions")
            else {
                report_peer(
                    p2p,
                    Some(peer_id.clone()),
                    PeerReportReason::MissingTransactions,
                );
                return None;
            };
            Some(SourcePeer { peer_id, data: txs })
        }
        None => {
            let Ok(SourcePeer { peer_id, data }) = p2p
                .get_transactions(range.clone())
                .await
                .trace_err("Failed to get transactions")
            else {
                return None;
            };
            let Some(txs) = data else {
                report_peer(
                    p2p,
                    Some(peer_id.clone()),
                    PeerReportReason::MissingTransactions,
                );
                return None;
            };
            Some(SourcePeer { peer_id, data: txs })
        }
    }
}

async fn get_headers_batch<P>(range: Range<u32>, p2p: &Arc<P>) -> SealedHeaderBatch
where
    P: PeerToPeerPort + Send + Sync + 'static,
{
    tracing::debug!(
        "getting header range from {} to {} inclusive",
        range.start,
        range.end
    );
    let Some(sourced_headers) = get_sealed_block_headers(range.clone(), p2p).await else {
        return Batch::new(None, range, vec![])
    };
    let SourcePeer {
        peer_id,
        data: headers,
    } = sourced_headers;
    let heights = range.clone().map(BlockHeight::from);
    let headers = headers
        .into_iter()
        .zip(heights)
        .take_while(move |(header, expected_height)| {
            let height = header.entity.height();
            height == expected_height
        })
        .map(|(header, _)| header)
        .collect::<Vec<_>>();
    if headers.len() != range.len() {
        report_peer(
            p2p,
            Some(peer_id.clone()),
            PeerReportReason::MissingBlockHeaders,
        );
    }
    Batch::new(Some(peer_id), range, headers)
}

fn report_peer<P>(p2p: &Arc<P>, peer_id: Option<PeerId>, reason: PeerReportReason)
where
    P: PeerToPeerPort + Send + Sync + 'static,
{
    if let Some(peer_id) = peer_id {
        tracing::info!("Reporting peer for {:?}", reason);

        // Failure to report a peer is a non-fatal error; ignore the error
        let _ = p2p
            .report_peer(peer_id.clone(), reason)
            .trace_err(&format!("Failed to report peer {:?}", peer_id));
    }
}

/// Get blocks correlating to the headers from a specific peer
#[tracing::instrument(skip(p2p, headers))]
async fn get_blocks<P>(p2p: &Arc<P>, headers: SealedHeaderBatch) -> SealedBlockBatch
where
    P: PeerToPeerPort + Send + Sync + 'static,
{
    let Batch {
        results: headers,
        range,
        peer,
    } = headers;

    let Some(SourcePeer {
        peer_id,
        data: transactions,
    }) = get_transactions(range.clone(), peer.clone(), p2p).await
    else {
        return Batch::new(peer, range, vec![])
    };

    let iter = headers.into_iter().zip(transactions.into_iter());
    let mut blocks = vec![];
    for (block_header, transactions) in iter {
        let SealedBlockHeader {
            consensus,
            entity: header,
        } = block_header;
        let block =
            Block::try_from_executed(header, transactions.0).map(|block| SealedBlock {
                entity: block,
                consensus,
            });
        if let Some(block) = block {
            blocks.push(block);
        } else {
            report_peer(
                p2p,
                Some(peer_id.clone()),
                PeerReportReason::InvalidTransactions,
            );
            break
        }
    }
    Batch::new(Some(peer_id), range, blocks)
}

#[tracing::instrument(
    skip_all,
    fields(
        height = **block.entity.header().height(),
        id = %block.entity.header().consensus().generated.application_hash
    ),
    err
)]
async fn execute_and_commit<E>(
    executor: &E,
    state: &SharedMutex<State>,
    block: SealedBlock,
) -> anyhow::Result<()>
where
    E: BlockImporterPort + Send + Sync + 'static,
{
    // Execute and commit the block.
    let height = *block.entity.header().height();
    let r = executor.execute_and_commit(block).await;

    // If the block executed successfully, mark it as committed.
    if let Err(err) = &r {
        tracing::error!("Execution of height {} failed: {:?}", *height, err);
    } else {
        state.apply(|s| s.commit(*height));
    }
    r
}

/// Extra stream utilities.
trait StreamUtil: Sized {
    /// Scan the stream for `None`.
    fn into_scan_none(self) -> ScanNone<Self> {
        ScanNone(self)
    }

    /// Scan the stream for errors.
    fn into_scan_err(self) -> ScanErr<Self> {
        ScanErr(self)
    }
}

impl<S> StreamUtil for S {}

struct ScanErr<S>(S);
struct ScanNone<S>(S);

impl<S> ScanNone<S> {
    fn scan_none<'a, T: 'a>(self) -> impl Stream<Item = T> + 'a
    where
        S: Stream<Item = Option<T>> + Send + 'a,
    {
        let stream = self.0.boxed::<'a>();
        futures::stream::unfold(stream, |mut stream| async move {
            let element = stream.next().await??;
            Some((element, stream))
        })
    }
}

impl<S> ScanErr<S> {
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
