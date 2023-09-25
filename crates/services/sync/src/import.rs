//! # Importer Task
//! This module contains the import task which is responsible for
//! importing blocks from the network into the local blockchain.

use futures::{
    stream,
    FutureExt,
    TryStreamExt,
};
use std::{
    future::Future,
    ops::RangeInclusive,
    sync::Arc,
};

use anyhow::anyhow;
use fuel_core_services::{
    SharedMutex,
    StateWatcher,
};
use fuel_core_types::{
    self,
    // blockchain::consensus::Sealed,
    fuel_types::BlockHeight,
    services::p2p::PeerId,
};
use fuel_core_types::{
    blockchain::primitives::BlockId,
    // fuel_tx::Transaction,
    services::p2p::Transactions,
};
use fuel_core_types::{
    blockchain::{
        block::Block,
        // consensus::Sealed,
        // header::BlockHeader,
        SealedBlock,
        SealedBlockHeader,
    },
    services::p2p::SourcePeer,
};
use futures::{
    stream::StreamExt,
    // FutureExt,
    Stream,
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

#[derive(Debug, derive_more::Display)]
enum ImportError {
    ConsensusError(anyhow::Error),
    ExecutionError(anyhow::Error),
    NoSuitablePeer,
    MissingBlockHeaders,
    MissingTransactions,
    BadBlockHeader,
    BlockHeightMismatch,
    JoinError(JoinError),
    Other(anyhow::Error),
}

impl From<anyhow::Error> for ImportError {
    fn from(value: anyhow::Error) -> Self {
        ImportError::Other(value)
    }
}

impl ImportError {
    /// All `ImportErrors` will stop the import stream. Fatal `ImportErrors`
    /// will prevent the notify signal at the end of the import. Non-fatal
    /// `ImportErrors` will allow the notify signal at the end of the import.
    fn is_fatal(&self) -> bool {
        #[allow(clippy::match_like_matches_macro)]
        match self {
            ImportError::BlockHeightMismatch => false,
            ImportError::BadBlockHeader => false,
            ImportError::MissingTransactions => false,
            _ => true,
        }
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
            let (count, result) = self.launch_stream(range.clone(), shutdown).await;

            // Get the size of the range.
            let range_len = range.size_hint().0 as u32;

            // If we did not process the entire range, mark the failed heights as failed.
            if (count as u32) < range_len {
                let incomplete_range = (*range.start() + count as u32)..=*range.end();
                tracing::error!(
                    "Failed to import range of blocks: {:?}",
                    incomplete_range
                );
                self.state.apply(|s| s.failed_to_process(incomplete_range));
            }
            result?;
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
    ) -> (usize, Result<(), ImportError>) {
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
        if let Err(err) = peer {
            let err = Err(err);
            return (0, err)
        }
        let peer = peer.expect("Checked");

        let context_generator =
            stream::repeat((peer.clone(), state.clone(), p2p.clone(), executor.clone()));

        let block_stream = get_block_stream(
            peer.clone(),
            range.clone(),
            params,
            p2p.clone(),
            consensus.clone(),
        );

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
                        blocks = stream_block_batch => blocks,
                        // If a shutdown signal is received during the stream, terminate early and
                        // return an empty response
                        _ = shutdown_signal.while_started() => (vec![], None)
                    }
                }).then(|task| async { task.map_err(ImportError::JoinError) })
            })
            // Request up to `block_stream_buffer_size` transactions from the network.
            .buffered(params.block_stream_buffer_size)
            // Continue the stream unless an error occurs.
            // Note the error will be returned but the stream will close.
            .into_scan_err()
            .scan_err()
            // Continue the stream until the shutdown signal is received.
            .take_until({
                let mut s = shutdown.clone();
                async move {
                    let _ = s.while_started().await;
                    tracing::info!("In progress import stream shutting down");
                }
            })
            // Then execute and commit the block
            .zip(context_generator)
            .then(
                |(blocks_result, (peer, state, p2p, executor))| async move {
                    let (sealed_blocks, error) = blocks_result?;
                    let sealed_blocks = futures::stream::iter(sealed_blocks);
                    let res = sealed_blocks.then(|sealed_block| async {
                        execute_and_commit(executor.as_ref(), &state, sealed_block).await
                    }).try_collect::<Vec<_>>().await.and_then(|v| error.map_or(Ok(v), Err));
                    match &res {
                        Ok(_) => {
                            report_peer(p2p.as_ref(), peer.clone(), PeerReportReason::SuccessfulBlockImport).await;
                        },
                        Err(e) => {
                            // If this fails, then it means that consensus has approved a block that is invalid.
                            // This would suggest a more serious issue than a bad peer, e.g. a fork or an out-of-date client.
                            tracing::error!("Failed to execute and commit block from peer {:?}: {:?}", peer, e);
                        },
                    }
                    res
                }
                .instrument(tracing::debug_span!("execute_and_commit"))
                .in_current_span()
            )
            // Continue the stream unless an error occurs.
            .into_scan_empty_or_err()
            .scan_empty_or_err()
            // Count the number of successfully executed blocks and
            // find any errors.
            // Fold the stream into a count and any errors.
            .fold((0usize, Ok(())), |(count, res), result| async move {
                match result {
                    Ok(_) => (count + 1, res),
                    Err(e) if !e.is_fatal() => (count, Ok(())),
                    Err(e) => (count, Err(e))
                }
            })
            .in_current_span()
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
) -> impl Stream<Item = impl Future<Output = (Vec<SealedBlock>, Option<ImportError>)>> {
    let Config {
        header_batch_size, ..
    } = params;
    let header_stream = get_header_stream(peer_id.clone(), range, params, p2p.clone());
    let generator =
        futures::stream::repeat((peer_id.clone(), p2p.clone(), consensus.clone()));
    let iter = header_stream.zip(generator.clone());
    let checked_headers = iter
        .then(|(header, (peer_id, p2p, consensus))| async move {
            let header = header?;
            check_sealed_header(&header, peer_id, p2p.as_ref(), consensus.as_ref())
                .await?;
            await_da_height(&header, consensus.as_ref()).await?;
            Ok(header)
        })
        .into_scan_err()
        .scan_err();
    checked_headers
        .chunks(*header_batch_size as usize)
        .zip(generator)
        .map(|(headers, (peer_id, p2p, ..))| get_blocks(p2p, peer_id, headers))
}

fn get_header_stream<P: PeerToPeerPort + Send + Sync + 'static>(
    peer: PeerId,
    range: RangeInclusive<u32>,
    params: &Config,
    p2p: Arc<P>,
) -> impl Stream<Item = Result<SealedBlockHeader, ImportError>> {
    let Config {
        header_batch_size, ..
    } = params;
    let ranges = range_chunks(range, *header_batch_size);
    futures::stream::iter(ranges)
        .then(move |range| {
            let p2p = p2p.clone();
            let peer = peer.clone();
            async move {
                tracing::debug!(
                    "getting header range from {} to {} inclusive",
                    range.start(),
                    range.end()
                );
                get_headers_batch(peer, range, p2p.as_ref()).await
            }
        })
        .flatten()
        .into_scan_err()
        .scan_err()
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

async fn check_sealed_header<
    P: PeerToPeerPort + Send + Sync + 'static,
    C: ConsensusPort + Send + Sync + 'static,
>(
    header: &SealedBlockHeader,
    peer_id: PeerId,
    p2p: &P,
    consensus_port: &C,
) -> Result<(), ImportError> {
    let validity = consensus_port
        .check_sealed_header(header)
        .map_err(ImportError::ConsensusError)
        .trace_err("Failed to check consensus on header")?;
    if validity {
        Ok(())
    } else {
        report_peer(p2p, peer_id.clone(), PeerReportReason::BadBlockHeader).await;
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
    range: RangeInclusive<u32>,
    p2p: &P,
) -> Result<Vec<SealedBlockHeader>, ImportError>
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
    let res = p2p
        .get_sealed_block_headers(peer.clone().bind(start..end))
        .await;
    let SourcePeer { data: headers, .. } = res;
    match headers {
        Ok(Some(headers)) => Ok(headers),
        Ok(None) => {
            report_peer(p2p, peer.clone(), PeerReportReason::MissingBlockHeaders).await;
            Err(ImportError::MissingBlockHeaders)
        }
        Err(e) => Err(e.into()),
    }
}

async fn get_transactions<P>(
    peer_id: PeerId,
    block_ids: Vec<BlockId>,
    p2p: &P,
) -> Result<Vec<Transactions>, ImportError>
where
    P: PeerToPeerPort + Send + Sync + 'static,
{
    let block_ids = peer_id.clone().bind(block_ids);
    let res = p2p.get_transactions_2(block_ids).await;
    match res {
        Ok(Some(transactions)) => Ok(transactions),
        Ok(None) => {
            report_peer(p2p, peer_id.clone(), PeerReportReason::MissingTransactions)
                .await;
            Err(ImportError::MissingTransactions)
        }
        Err(e) => Err(e.into()),
    }
}

async fn get_headers_batch<P>(
    peer_id: PeerId,
    range: RangeInclusive<u32>,
    p2p: &P,
) -> impl Stream<Item = Result<SealedBlockHeader, ImportError>>
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
    let res = get_sealed_block_headers(peer_id.clone(), range.clone(), p2p).await;
    let headers = match res {
        Ok(headers) => {
            let headers = headers.into_iter();
            let heights = range.map(BlockHeight::from);
            let headers = headers
                .zip(heights)
                .map(move |(header, expected_height)| {
                    let height = *header.entity.height();
                    if height == expected_height {
                        Ok(header)
                    } else {
                        Err(ImportError::BlockHeightMismatch)
                    }
                })
                .collect::<Vec<_>>();
            if let Some(expected_len) = end.checked_sub(start) {
                if headers.len() != expected_len as usize
                    || headers.iter().any(|h| h.is_err())
                {
                    report_peer(
                        p2p,
                        peer_id.clone(),
                        PeerReportReason::MissingBlockHeaders,
                    )
                    .await;
                }
            }
            headers
        }
        Err(e) => {
            let error = Err(e);
            vec![error]
        }
    };
    futures::stream::iter(headers)
}

async fn report_peer<P>(p2p: &P, peer_id: PeerId, reason: PeerReportReason)
where
    P: PeerToPeerPort + Send + Sync + 'static,
{
    // Failure to report a peer is a non-fatal error; ignore the error
    let _ = p2p
        .report_peer(peer_id.clone(), reason)
        .await
        .map_err(|e| tracing::error!("Failed to report peer {:?}: {:?}", peer_id, e));
}

// Get blocks correlating to the headers from a specific peer
// #[tracing::instrument(
//     skip(p2p, headers),
//     fields(
//         height = **header.data.height(),
//         id = %header.data.consensus.generated.application_hash
//     ),
//     err
// )]
async fn get_blocks<P>(
    p2p: Arc<P>,
    peer_id: PeerId,
    headers: Vec<Result<SealedBlockHeader, ImportError>>,
) -> (Vec<SealedBlock>, Option<ImportError>)
where
    P: PeerToPeerPort + Send + Sync + 'static,
{
    let (headers, errors): (Vec<_>, Vec<_>) =
        headers.into_iter().partition(|r| r.is_ok());
    let headers = headers
        .into_iter()
        .map(|item| item.expect("Result is checked for Ok"))
        .collect::<Vec<_>>();
    let mut errors = errors
        .into_iter()
        .map(|item| item.expect_err("Result is checked for Error"))
        .collect::<Vec<_>>();
    let mut err = errors.pop();
    if headers.is_empty() {
        return (vec![], err)
    }

    let block_ids = headers.iter().map(|header| header.entity.id()).collect();
    let maybe_txs = get_transactions(peer_id.clone(), block_ids, p2p.as_ref()).await;
    match maybe_txs {
        Ok(transaction_data) => {
            let headers = headers;
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
                    tracing::error!(
                        "Failed to created block from header and transactions"
                    );
                    report_peer(
                        p2p.as_ref(),
                        peer_id.clone(),
                        PeerReportReason::InvalidTransactions,
                    )
                    .await;
                }
            }
            (blocks, err)
        }
        Err(error) => {
            // Failure to retrieve transactions due to a networking error,
            // invalid response, or any other reason constitutes a fatal error.
            err = Some(error);
            (vec![], err)
        }
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
    /// Turn a stream of `Result<Option<T>>` into a stream of `Result<T>`.
    /// Close the stream if an error occurs or a `None` is received.
    /// Return the error if the stream closes.
    // fn into_scan_none_or_err(self) -> ScanNoneErr<Self> {
    //     ScanNoneErr(self)
    // }

    /// Close the stream if an error occurs or an empty `Vector<T>` is received.
    /// Return the error if the stream closes.
    fn into_scan_empty_or_err(self) -> ScanEmptyErr<Self> {
        ScanEmptyErr(self)
    }

    /// Turn a stream of `Result<T>` into a stream of `Result<T>`.
    /// Close the stream if an error occurs.
    /// Return the error if the stream closes.
    fn into_scan_err(self) -> ScanErr<Self> {
        ScanErr(self)
    }
}

impl<S> StreamUtil for S {}

// struct ScanNoneErr<S>(S);
struct ScanEmptyErr<S>(S);
struct ScanErr<S>(S);

// impl<S> ScanNoneErr<S> {
//     /// Scan the stream for `None` or errors.
//     fn scan_none_or_err<R>(self) -> impl Stream<Item = Result<R, ImportError>>
//     where
//         S: Stream<Item = Result<Option<R>, ImportError>> + Send + 'static,
//     {
//         let stream = self.0.boxed();
//         futures::stream::unfold((false, stream), |(mut is_err, mut stream)| async move {
//             if is_err {
//                 None
//             } else {
//                 let result = stream.next().await?;
//                 is_err = result.is_err();
//                 result.transpose().map(|result| (result, (is_err, stream)))
//             }
//         })
//     }
// }

impl<S> ScanEmptyErr<S> {
    /// Scan the stream for empty vector or errors.
    fn scan_empty_or_err<R>(self) -> impl Stream<Item = Result<Vec<R>, ImportError>>
    where
        S: Stream<Item = Result<Vec<R>, ImportError>> + Send + 'static,
    {
        let stream = self.0.boxed();
        futures::stream::unfold((false, stream), |(mut is_err, mut stream)| async move {
            if is_err {
                None
            } else {
                let result = stream.next().await?;
                is_err = result.is_err();
                result
                    .map(|v| (!v.is_empty()).then_some(v))
                    .transpose()
                    .map(|result| (result, (is_err, stream)))
            }
        })
    }
}

impl<S> ScanErr<S> {
    /// Scan the stream for errors.
    fn scan_err<R>(self) -> impl Stream<Item = Result<R, ImportError>>
    where
        S: Stream<Item = Result<R, ImportError>> + Send + 'static,
    {
        let stream = self.0.boxed();
        futures::stream::unfold((false, stream), |(mut err, mut stream)| async move {
            if err {
                None
            } else {
                let result = stream.next().await?;
                err = result.is_err();
                Some((result, (err, stream)))
            }
        })
    }
}
