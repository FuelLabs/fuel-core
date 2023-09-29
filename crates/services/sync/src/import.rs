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
    pub fn new(peer: PeerId, range: Range<u32>, results: Vec<T>) -> Self {
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

#[derive(Debug, derive_more::Display)]
enum ImportError {
    ConsensusError(anyhow::Error),
    ExecutionError(anyhow::Error),
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

        let block_stream =
            get_block_stream(range.clone(), params, p2p.clone(), consensus.clone());
        let result = block_stream
            .map(move |stream_block_batch| {
                let shutdown_guard = shutdown_guard.clone();
                let shutdown_signal = shutdown_signal.clone();
                tokio::spawn(async move {
                    // Hold a shutdown sender for the lifetime of the spawned task
                    let _shutdown_guard = shutdown_guard.clone();
                    let mut shutdown_signal = shutdown_signal.clone();
                    tokio::select! {
                    // Stream a batch of blocks
                    blocks = stream_block_batch => blocks.map(Some),
                    // If a shutdown signal is received during the stream, terminate early and
                    // return an empty response
                    _ = shutdown_signal.while_started() => Ok(None)
                }
                }).map(|task| task.map_err(ImportError::JoinError)?)
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
            .into_scan_none_or_err()
            .scan_none_or_err()
            .then(|batch| {
                async move {
                    let batch = batch?;
                    let error = batch.is_err().then(|| ImportError::MissingTransactions);
                    let Batch {
                        peer,
                        results,
                        ..
                    } = batch;
                    let sealed_blocks = futures::stream::iter(results);
                    let res = sealed_blocks.then(|sealed_block| async {
                        execute_and_commit(executor.as_ref(), state, sealed_block).await
                    }).try_collect::<Vec<_>>().await.and_then(|v| error.map_or(Ok(v), Err));
                    match &res {
                        Ok(_) => {
                            report_peer(p2p.as_ref(), peer.clone(), PeerReportReason::SuccessfulBlockImport);
                        },
                        Err(e) => {
                            // If this fails, then it means that consensus has approved a block that is invalid.
                            // This would suggest a more serious issue than a bad peer, e.g. a fork or an out-of-date client.
                            tracing::error!("Failed to execute and commit block from peer {:?}: {:?}", peer, e);
                        },
                    };
                    res
                }
                .instrument(tracing::debug_span!("execute_and_commit"))
                .in_current_span()
            })
            // Continue the stream unless an error occurs.
            .into_scan_err()
            .scan_err()
            // Count the number of successfully executed blocks and
            // find any errors.
            // Fold the stream into a count and any errors.
            .fold(0usize, |count, result| async move {
                match result {
                    Ok(batch) => count + batch.len(),
                    Err(_) => count
                }
            })
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
    range: RangeInclusive<u32>,
    params: &Config,
    p2p: Arc<P>,
    consensus: Arc<C>,
) -> impl Stream<Item = impl Future<Output = Result<SealedBlockBatch, ImportError>>> + '_
{
    let header_stream = get_header_batch_stream(range.clone(), params, p2p.clone());
    header_stream
        .map({
            let consensus = consensus.clone();
            let p2p = p2p.clone();
            move |header_batch| {
                let header_batch = header_batch?;
                let Batch {
                    peer,
                    range,
                    results,
                } = header_batch;
                let results = results
                    .into_iter()
                    .map({
                        let consensus = consensus.clone();
                        let p2p = p2p.clone();
                        let peer = peer.clone();
                        move |header| {
                            check_sealed_header(
                                &header,
                                peer.clone(),
                                p2p.clone(),
                                consensus.clone(),
                            )?;
                            Result::<_, ImportError>::Ok(header)
                        }
                    })
                    .take_while(|result| result.is_ok())
                    .filter_map(|result| result.ok())
                    .collect::<Vec<_>>();
                let batch = Batch::new(peer.clone(), range, results);
                Result::<_, ImportError>::Ok(batch)
            }
        })
        .map({
            let consensus = consensus.clone();
            move |valid_headers_batch| {
                let consensus = consensus.clone();
                async move {
                    let valid_headers = valid_headers_batch?;
                    if let Some(header) = valid_headers.results.last() {
                        await_da_height(header, consensus.as_ref()).await?
                    };
                    Result::<_, ImportError>::Ok(valid_headers)
                }
            }
        })
        .map({
            let p2p = p2p.clone();
            move |headers| {
                let p2p = p2p.clone();
                async move {
                    let headers = headers.await?;
                    let Batch {
                        peer,
                        range,
                        results,
                    } = headers;
                    if results.is_empty() {
                        let batch = SealedBlockBatch::new(peer, range, vec![]);
                        Ok(batch)
                    } else {
                        let headers =
                            SealedHeaderBatch::new(peer.clone(), range.clone(), results);
                        let batch = get_blocks(p2p, headers).await?;
                        Ok(batch)
                    }
                }
                .instrument(tracing::debug_span!("consensus_and_transactions"))
                .in_current_span()
            }
        })
}

fn get_header_batch_stream<P: PeerToPeerPort + Send + Sync + 'static>(
    range: RangeInclusive<u32>,
    params: &Config,
    p2p: Arc<P>,
) -> impl Stream<Item = Result<SealedHeaderBatch, ImportError>> {
    let Config {
        header_batch_size, ..
    } = params;
    let ranges = range_chunks(range, *header_batch_size);
    futures::stream::iter(ranges).then(move |range| {
        let p2p = p2p.clone();
        async move { get_headers_batch(range, p2p).await }
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

async fn get_sealed_block_headers<P>(
    range: Range<u32>,
    p2p: &P,
) -> Result<SourcePeer<Vec<SealedBlockHeader>>, ImportError>
where
    P: PeerToPeerPort + Send + Sync + 'static,
{
    tracing::debug!(
        "getting header range from {} to {} inclusive",
        range.start,
        range.end
    );
    let res = p2p
        .get_sealed_block_headers(range)
        .await
        .trace_err("Failed to get headers");
    let sorted_headers = match res {
        Ok(sourced_headers) => {
            let sourced = sourced_headers.map(|headers| match headers {
                None => vec![],
                Some(headers) => headers,
            });
            Ok(sourced)
        }
        Err(e) => Err(ImportError::Other(e)),
    };
    sorted_headers
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
    range: RangeInclusive<u32>,
    p2p: Arc<P>,
) -> Result<SealedHeaderBatch, ImportError>
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
    let result = get_sealed_block_headers(range.clone(), p2p.as_ref()).await;
    let sourced_headers = result?;
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
    if let Some(expected_len) = end.checked_sub(start) {
        if headers.len() != expected_len as usize {
            report_peer(
                p2p.as_ref(),
                peer_id.clone(),
                PeerReportReason::MissingBlockHeaders,
            );
        }
    }
    Ok(Batch::new(peer_id, range.clone(), headers))
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
async fn get_blocks<P>(
    p2p: Arc<P>,
    headers: SealedHeaderBatch,
) -> Result<SealedBlockBatch, ImportError>
where
    P: PeerToPeerPort + Send + Sync + 'static,
{
    let Batch {
        results: headers,
        peer,
        range,
    } = headers;
    let transaction_data =
        get_transactions(peer.clone(), range.clone(), p2p.as_ref()).await?;
    let iter = headers.into_iter().zip(transaction_data);
    let mut blocks = vec![];
    for (block_header, transactions) in iter {
        let block_header = block_header.clone();
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
                p2p.as_ref(),
                peer.clone(),
                PeerReportReason::InvalidTransactions,
            );
        }
    }
    Ok(Batch::new(peer, range, blocks))
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
    fn into_scan_none_or_err(self) -> ScanNoneErr<Self> {
        ScanNoneErr(self)
    }

    fn into_scan_err(self) -> ScanErr<Self> {
        ScanErr(self)
    }
}

impl<S> StreamUtil for S {}

struct ScanErr<S>(S);
struct ScanNoneErr<S>(S);

impl<S> ScanNoneErr<S> {
    /// Scan the stream for `None` or errors.
    fn scan_none_or_err<'a, T: 'a>(
        self,
    ) -> impl Stream<Item = Result<T, ImportError>> + 'a
    where
        S: Stream<Item = Result<Option<T>, ImportError>> + Send + 'a,
    {
        let stream = self.0.boxed();
        futures::stream::unfold((false, stream), |(mut is_err, mut stream)| async move {
            if is_err {
                None
            } else {
                let result = stream.next().await?;
                is_err = result.is_err();
                result.transpose().map(|result| (result, (is_err, stream)))
            }
        })
    }
}

impl<S> ScanErr<S> {
    /// Scan the stream for errors.
    fn scan_err<'a, T: 'a>(self) -> impl Stream<Item = Result<T, ImportError>> + 'a
    where
        S: Stream<Item = Result<T, ImportError>> + Send + 'a,
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
