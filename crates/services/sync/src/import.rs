//! # Importer Task
//! This module contains the import task which is responsible for
//! importing blocks from the network into the local blockchain.

use std::{
    ops::RangeInclusive,
    sync::Arc,
};

use fuel_core_services::{
    SharedMutex,
    StateWatcher,
};
use fuel_core_types::{
    blockchain::{
        block::Block,
        consensus::Sealed,
        primitives::BlockId,
        SealedBlock,
        SealedBlockHeader,
    },
    fuel_types::BlockHeight,
    services::p2p::SourcePeer,
};
use futures::{
    stream::StreamExt,
    Stream,
};
use tokio::sync::Notify;
use tracing::Instrument;

use crate::{
    ports::{
        BlockImporterPort,
        ConsensusPort,
        PeerToPeerPort,
    },
    state::State,
    tracing_helpers::{
        TraceErr,
        TraceNone,
    },
};

#[cfg(test)]
pub(crate) use tests::empty_header;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod back_pressure_tests;

#[derive(Clone, Copy, Debug)]
/// Parameters for the import task.
pub struct Config {
    /// The maximum number of get header requests to make in a single batch.
    pub max_get_header_requests: usize,
    /// The maximum number of get transaction requests to make in a single batch.
    pub max_get_txns_requests: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_get_header_requests: 10,
            max_get_txns_requests: 10,
        }
    }
}

pub(crate) struct Import<P, E, C> {
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
    pub(crate) fn new(
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
}
impl<P, E, C> Import<P, E, C>
where
    P: PeerToPeerPort + Send + Sync + 'static,
    E: BlockImporterPort + Send + Sync + 'static,
    C: ConsensusPort + Send + Sync + 'static,
{
    #[tracing::instrument(skip_all)]
    pub(crate) async fn import(
        &self,
        shutdown: &mut StateWatcher,
    ) -> anyhow::Result<bool> {
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

        let headers_res = get_header_range_single_req(range.clone(), p2p.clone()).await;
        let unchecked_headers = match headers_res {
            Ok(headers) => headers,
            Err(e) => return (0, Err(e)),
        };
        let headers = check_headers_match_range(&unchecked_headers, range.clone());

        futures::stream::iter(headers)
        .map({
            let p2p = p2p.clone();
            let consensus_port = consensus.clone();
            move |result| {
                let p2p = p2p.clone();
                let consensus_port = consensus_port.clone();
                Self::get_block_for_header(result, p2p, consensus_port)
            }
            .instrument(tracing::debug_span!("consensus_and_transactions"))
            .in_current_span()
        })
        // Request up to `max_get_txns_requests` transactions from the network.
        .buffered(params.max_get_txns_requests)
        // Continue the stream unless an error or none occurs.
        // Note the error will be returned but the stream will close.
        .into_scan_none_or_err()
        .scan_none_or_err()
        // Continue the stream until the shutdown signal is received.
        .take_until({
            let mut s = shutdown.clone();
            async move {
                let _ = s.while_started().await;
                tracing::info!("In progress import stream shutting down");
            }
        })
        .then({
            let state = state.clone();
            let executor = executor.clone();
            move |block| {
                let state = state.clone();
                let executor = executor.clone();
                async move {
                    // Short circuit on error.
                    let block = match block {
                        Ok(b) => b,
                        Err(e) => return Err(e),
                    };

                    execute_and_commit(executor.as_ref(), &state, block).await
                }
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
        .fold((0usize, Ok(())), |(count, err), result| async move {
            match result {
                Ok(_) => (count + 1, err),
                Err(e) => (count, Err(e)),
            }
        })
        .in_current_span()
        .await
    }

    async fn get_block_for_header(
        header: SourcePeer<SealedBlockHeader>,
        p2p: Arc<P>,
        consensus_port: Arc<C>,
    ) -> anyhow::Result<Option<SealedBlock>> {
        let SourcePeer {
            peer_id,
            data: header,
        } = header;
        let id = header.entity.id();
        let block_id = SourcePeer { peer_id, data: id };

        // Check the consensus is valid on this header.
        if !consensus_port
            .check_sealed_header(&header)
            .trace_err("Failed to check consensus on header")?
        {
            tracing::warn!("Header {:?} failed consensus check", header);
            return Ok(None)
        }

        // Wait for the da to be at least the da height on the header.
        consensus_port
            .await_da_height(&header.entity.da_height)
            .await?;

        get_transactions_on_block(p2p.as_ref(), block_id, header).await
    }
}

// The response from the P2P network isn't guaranteed to include all the
fn check_headers_match_range(
    headers: &Vec<SourcePeer<SealedBlockHeader>>,
    range: RangeInclusive<u32>,
) -> Vec<SourcePeer<SealedBlockHeader>> {
    let mut new_headers = Vec::new();
    let mut header_iter = headers.iter();
    for height in range {
        if let Some(header) =
            header_iter.find(|h| h.data.entity.height() == &height.into())
        {
            new_headers.push(header.clone());
        } else {
            break
        }
    }
    new_headers
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

async fn get_header_range_single_req(
    range: RangeInclusive<u32>,
    p2p: Arc<impl PeerToPeerPort + 'static>,
) -> anyhow::Result<Vec<SourcePeer<SealedBlockHeader>>> {
    let start = *range.start();
    let end = *range.end();
    p2p.get_sealed_block_headers_inclusive(start.into(), end.into())
        .await
}

#[tracing::instrument(
    skip(p2p, header),
    fields(
        height = **header.entity.height(),
        id = %header.entity.consensus.generated.application_hash
    ),
    err
)]
async fn get_transactions_on_block<P>(
    p2p: &P,
    block_id: SourcePeer<BlockId>,
    header: SealedBlockHeader,
) -> anyhow::Result<Option<SealedBlock>>
where
    P: PeerToPeerPort + Send + Sync + 'static,
{
    let Sealed {
        entity: header,
        consensus,
    } = header;

    // Request the transactions for this block.
    Ok(p2p
        .get_transactions(block_id)
        .await
        .trace_err("Failed to get transactions")?
        .trace_none_warn("Could not find transactions for header")
        .and_then(|transactions| {
            let block = Block::try_from_executed(header, transactions)
                .trace_none_warn("Failed to created header from executed transactions")?;
            Some(SealedBlock {
                entity: block,
                consensus,
            })
        }))
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

    /// Turn a stream of `Result<T>` into a stream of `Result<T>`.
    /// Close the stream if an error occurs.
    /// Return the error if the stream closes.
    fn into_scan_err(self) -> ScanErr<Self> {
        ScanErr(self)
    }
}

impl<S> StreamUtil for S {}

struct ScanNoneErr<S>(S);
struct ScanErr<S>(S);

impl<S> ScanNoneErr<S> {
    /// Scan the stream for `None` or errors.
    fn scan_none_or_err<R>(self) -> impl Stream<Item = anyhow::Result<R>>
    where
        S: Stream<Item = anyhow::Result<Option<R>>> + Send + 'static,
    {
        let stream = self.0.boxed();
        futures::stream::unfold((false, stream), |(mut err, mut stream)| async move {
            if err {
                None
            } else {
                let result = stream.next().await?;
                err = result.is_err();
                result.transpose().map(|result| (result, (err, stream)))
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

// #[cfg(test)]
// mod tests {
//     use crate::ports::MockPeerToPeerPort;
//
//     #[test]
//     fn get_header_range() {
//         let p2p = MockPeerToPeerPort
//     }
// }
