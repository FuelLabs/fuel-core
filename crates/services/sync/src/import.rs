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
use tokio::sync::Notify;
use tracing::Instrument;

use crate::{
    ports::{
        BlockImporterPort,
        ConsensusPort,
        PeerReportReason,
        PeerToPeerPort,
    },
    state::State,
    tracing_helpers::{
        TraceErr,
        TraceNone,
    },
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

impl<P, E, C> Import<P, E, C>
where
    P: PeerToPeerPort + Send + Sync + 'static,
    E: BlockImporterPort + Send + Sync + 'static,
    C: ConsensusPort + Send + Sync + 'static,
{
    #[tracing::instrument(skip_all)]
    /// Execute imports until a shutdown is requested.
    pub async fn import(&self, shutdown: &mut StateWatcher) -> anyhow::Result<bool> {
        self.import_inner(shutdown).await?;

        Ok(wait_for_notify_or_shutdown(&self.notify, shutdown).await)
    }

    async fn import_inner(&self, shutdown: &StateWatcher) -> anyhow::Result<()> {
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
    ) -> (usize, anyhow::Result<()>) {
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
        let peer = p2p.select_peer(block_height).await;

        if let Err(err) = peer {
            let err = Err(err);
            return (0, err)
        }

        let peer = peer.expect("Checked");

        if let None = peer {
            let err = Err(anyhow!("Expected peer"));
            return (0, err)
        }

        let peer = peer.expect("Checked");

        let generator =
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
                    blocks = stream_block_batch => {
                        dbg!(&blocks);
                        blocks
                    },
                    // If a shutdown signal is received during the stream, terminate early and
                    // return an empty response
                    _ = shutdown_signal.while_started() => Ok(vec![])
                }
            }).then(|task| async { task.map_err(|e| anyhow!(e))? })
        })
        // Request up to `block_stream_buffer_size` transactions from the network.
        .buffered(params.block_stream_buffer_size)
        // Continue the stream unless an error or empty batch occurs.
        // Note the error will be returned but the stream will close.
        .into_scan_empty_or_err()
        .scan_empty_or_err()
        // Continue the stream until the shutdown signal is received.
        .take_until({
            let mut s = shutdown.clone();
            async move {
                let _ = s.while_started().await;
                tracing::info!("In progress import stream shutting down");
            }
        })
            .zip(generator)
        // Then execute and commit the block
        .then(
            |(res, (peer, state, p2p, executor))| async move {
                let sealed_blocks = res?;
                let sealed_blocks = futures::stream::iter(sealed_blocks);
                let res = sealed_blocks.then(|sealed_block| async {
                    execute_and_commit(executor.as_ref(), &state, sealed_block).await
                }).try_collect::<Vec<_>>().await;
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
        .into_scan_err()
        .scan_err()
        // Count the number of successfully executed blocks and
        // find any errors.
        // Fold the stream into a count and any errors.
        .fold((0usize, Ok(())), |(count, res), result| async move {
            match result {
                Ok(_) => (count + 1, res),
                Err(e) => (count, Err(e)),
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
    peer: PeerId,
    range: RangeInclusive<u32>,
    params: &Config,
    p2p: Arc<P>,
    consensus: Arc<C>,
) -> impl Stream<Item = impl Future<Output = anyhow::Result<Vec<SealedBlock>>>> {
    let Config {
        header_batch_size, ..
    } = params;
    let header_stream = get_header_stream(peer.clone(), range, params, p2p.clone());
    let generator =
        futures::stream::repeat((peer.clone(), p2p.clone(), consensus.clone()));
    let iter = header_stream.zip(generator.clone());
    iter.then(|(header, (peer, p2p, consensus))| async move {
        let header = header?;
        let validity =
            check_sealed_header(&header, peer, p2p.clone(), consensus.clone()).await?;
        if validity {
            consensus.await_da_height(&header.entity.da_height).await?;
            Ok(Some(header))
        } else {
            Ok(None)
        }
    })
    .into_scan_none_or_err()
    .scan_none_or_err()
    .chunks(*header_batch_size as usize)
    .zip(generator)
    .map(|(headers, (peer, p2p, ..))| {
        {
            let headers = peer.bind(headers);
            get_blocks(p2p, headers)
        }
        .instrument(tracing::debug_span!("consensus_and_transactions"))
        .in_current_span()
    })
}

fn get_header_stream<P: PeerToPeerPort + Send + Sync + 'static>(
    peer: PeerId,
    range: RangeInclusive<u32>,
    params: &Config,
    p2p: Arc<P>,
) -> impl Stream<Item = anyhow::Result<SealedBlockHeader>> {
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
        .into_scan_none_or_err()
        .scan_none_or_err()
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
    p2p: Arc<P>,
    consensus_port: Arc<C>,
) -> anyhow::Result<bool> {
    let validity = consensus_port
        .check_sealed_header(header)
        .trace_err("Failed to check consensus on header")?;
    if !validity {
        report_peer(
            p2p.as_ref(),
            peer_id.clone(),
            PeerReportReason::BadBlockHeader,
        )
        .await;
    }
    Ok(validity)
}

// async fn get_sealed_blocks<
//     P: PeerToPeerPort + Send + Sync + 'static,
//     C: ConsensusPort + Send + Sync + 'static,
// >(
//     headers: SourcePeer<Vec<anyhow::Result<SealedBlockHeader>>>,
//     p2p: Arc<P>,
//     _consensus: Arc<C>,
// ) -> anyhow::Result<Vec<SealedBlock>> {
//     get_blocks(p2p.as_ref(), headers).await
// }

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

async fn get_headers_batch<P>(
    peer: PeerId,
    mut range: RangeInclusive<u32>,
    p2p: &P,
) -> impl Stream<Item = anyhow::Result<Option<SealedBlockHeader>>>
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
        .get_sealed_block_headers(peer.bind(start..end))
        .await
        .trace_err("Failed to get headers");
    let headers = match res {
        Ok(sourced_headers) => {
            let SourcePeer {
                peer_id,
                data: maybe_headers,
            } = sourced_headers;
            let headers = match maybe_headers {
                None => {
                    tracing::error!(
                        "No headers received from peer {:?} for range {} to {}",
                        peer_id,
                        start,
                        end
                    );
                    vec![Err(anyhow::anyhow!("Headers provider was unable to fulfill request for unspecified reason. Possibly because requested batch size was too large"))]
                }
                Some(headers) => headers
                    .into_iter()
                    .map(move |header| {
                        let header = range.next().and_then(|height| {
                            if *(header.entity.height()) == height.into() {
                                Some(header)
                            } else {
                                None
                            }
                        });
                        Ok(header)
                    })
                    .collect(),
            };
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
            dbg!(&e);
            vec![Err(e)]
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
#[tracing::instrument(
    skip(p2p, headers),
    // fields(
    //     height = **header.data.height(),
    //     id = %header.data.consensus.generated.application_hash
    // ),
    err
)]
async fn get_blocks<P>(
    p2p: Arc<P>,
    headers: SourcePeer<Vec<anyhow::Result<SealedBlockHeader>>>,
) -> anyhow::Result<Vec<SealedBlock>>
where
    P: PeerToPeerPort + Send + Sync + 'static,
{
    // Get transactions for the set of valid block ids
    // Return the error as well if there is one

    let SourcePeer {
        peer_id,
        data: headers,
    } = headers;
    let (headers, _err): (Vec<_>, Vec<_>) =
        headers.into_iter().partition(|item| item.is_err());
    let headers = headers
        .into_iter()
        .filter_map(|header| header.ok())
        .collect::<Vec<_>>();
    let block_ids = headers.iter().map(|header| header.entity.id()).collect();
    let block_ids = peer_id.clone().bind(block_ids);
    let maybe_txs = p2p
        .get_transactions_2(block_ids)
        .await
        .trace_err("Failed to get transactions")?
        .trace_none_warn("Could not find transactions for header");

    match maybe_txs {
        None => {
            report_peer(
                p2p.as_ref(),
                peer_id.clone(),
                PeerReportReason::MissingTransactions,
            )
            .await;
            Ok(vec![])
        }
        Some(transaction_data) => {
            let headers = headers;
            let iter = headers.into_iter().zip(transaction_data);
            let mut blocks = vec![];
            for (block_header, transactions) in iter {
                let SealedBlockHeader {
                    consensus,
                    entity: header,
                } = block_header;
                let block = Block::try_from_executed(header, transactions).map(|block| {
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
            Ok(blocks)
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
) -> anyhow::Result<()>
where
    E: BlockImporterPort + Send + Sync + 'static,
{
    // Execute and commit the block.
    let height = *block.entity.header().height();
    let r = executor.execute_and_commit(block).await;

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
    fn into_scan_none_or_err(self) -> ScanNoneErr<Self> {
        ScanNoneErr(self)
    }

    /// Close the stream if an error occurs or a `None` is received.
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

struct ScanNoneErr<S>(S);
struct ScanEmptyErr<S>(S);
struct ScanErr<S>(S);

impl<S> ScanNoneErr<S> {
    /// Scan the stream for `None` or errors.
    fn scan_none_or_err<R>(self) -> impl Stream<Item = anyhow::Result<R>>
    where
        S: Stream<Item = anyhow::Result<Option<R>>> + Send + 'static,
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

#[cfg(test)]
mod test {
    use crate::import::StreamUtil;
    use anyhow::anyhow;
    use futures::{
        StreamExt,
        TryStreamExt,
    };

    #[tokio::test]
    async fn test_it() {
        let i = [Ok(Some(0)), Ok(Some(1)), Ok(None)];
        let stream = futures::stream::iter(i)
            .into_scan_none_or_err()
            .scan_none_or_err();
        let output = stream.collect::<Vec<_>>().await;
        println!("{:?}", output);
    }

    #[tokio::test]
    async fn test_it_2() {
        let i = [Ok(Some(0)), Ok(Some(1)), Err(anyhow!("err!"))];
        let stream = futures::stream::iter(i)
            .into_scan_none_or_err()
            .scan_none_or_err();
        let output = stream.try_collect::<Vec<_>>().await;
        assert!(output.is_err());
    }
}

impl<S> ScanEmptyErr<S> {
    /// Scan the stream for empty vector or errors.
    fn scan_empty_or_err<R>(self) -> impl Stream<Item = anyhow::Result<Vec<R>>>
    where
        S: Stream<Item = anyhow::Result<Vec<R>>> + Send + 'static,
    {
        let stream = self.0.boxed();
        futures::stream::unfold((false, stream), |(mut is_err, mut stream)| async move {
            if is_err {
                None
            } else {
                let result = stream.next().await?;
                is_err = result.is_err();
                result
                    .map(|v| (!v.is_empty()).then(|| v))
                    .transpose()
                    .map(|result| (result, (is_err, stream)))
            }
        })
    }
}

impl<S> ScanErr<S> {
    /// Scan the stream for errors.
    fn scan_err<R>(self) -> impl Stream<Item = anyhow::Result<R>>
    where
        S: Stream<Item = anyhow::Result<R>> + Send + 'static,
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
