use std::{
    ops::RangeInclusive,
    sync::Arc,
};

use fuel_core_services::{
    SharedMutex,
    Shutdown,
    SourcePeer,
};
use fuel_core_types::blockchain::{
    block::Block,
    consensus::Sealed,
    primitives::BlockHeight,
    SealedBlock,
    SealedBlockHeader,
};
use futures::{
    stream::{
        self,
        Scan,
        StreamExt,
    },
    Stream,
};
use std::future::Future;
use tokio::sync::Notify;

use crate::{
    ports::{
        ConsensusPort,
        ExecutorPort,
        PeerToPeerPort,
        Ports,
    },
    state::State,
};

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub max_get_header_requests: usize,
    pub max_get_txns_requests: usize,
}

pub(super) async fn import<P, E, C>(
    state: SharedMutex<State>,
    notify: Arc<Notify>,
    params: Params,
    ports: Ports<P, E, C>,
    shutdown: Shutdown,
    #[cfg(test)] mut loop_callback: impl FnMut(),
) -> anyhow::Result<bool>
where
    P: PeerToPeerPort + Send + Sync + 'static,
    E: ExecutorPort + Send + Sync + 'static,
    C: ConsensusPort + Send + Sync + 'static,
{
    loop {
        if let Some(range) = state.apply(|s| s.process_range()) {
            let (count, result) =
                get_header_range_buffered(range.clone(), params, ports.p2p.clone())
                    .map({
                        let p2p = ports.p2p.clone();
                        let consensus_port = ports.consensus.clone();
                        move |result| {
                            let p2p = p2p.clone();
                            let consensus_port = consensus_port.clone();
                            async move {
                                let header = match result {
                                    Ok(h) => h,
                                    Err(e) => return Err(e),
                                };
                                let SourcePeer {
                                    peer_id,
                                    data: header,
                                } = header;
                                let id = header.entity.id();
                                let block_id = SourcePeer { peer_id, data: id };
                                if !consensus_port.check_sealed_header(&header).await? {
                                    return Ok(None)
                                }
                                let Sealed {
                                    entity: header,
                                    consensus,
                                } = header;
                                Ok(p2p.get_transactions(block_id).await?.and_then(
                                    |transactions| {
                                        Some(SealedBlock {
                                            entity: Block::try_from_executed(
                                                header,
                                                transactions,
                                            )?,
                                            consensus,
                                        })
                                    },
                                ))
                            }
                        }
                    })
                    .buffered(params.max_get_txns_requests)
                    .into_scan_none_or_err()
                    .scan_none_or_err()
                    .take_until({
                        let s = shutdown.clone();
                        async move { s.wait().await }
                    })
                    .then({
                        let state = state.clone();
                        let executor = ports.executor.clone();
                        move |block| {
                            let state = state.clone();
                            let executor = executor.clone();
                            async move {
                                let block = match block {
                                    Ok(b) => b,
                                    Err(e) => return Err(e),
                                };
                                let height = *block.entity.header().height();
                                let r = executor.execute_and_commit(block).await;
                                if r.is_ok() {
                                    state.apply(|s| s.commit(*height))
                                }
                                r
                            }
                        }
                    })
                    .into_scan_err()
                    .scan_err()
                    .fold((0usize, Ok(())), |(count, err), result| async move {
                        match result {
                            Ok(_) => (count + 1, err),
                            Err(e) => (count, Err(e)),
                        }
                    })
                    .await;

            let range_len = range.size_hint().0 as u32;
            if (count as u32) < range_len {
                let range = (*range.start() + count as u32)..=*range.end();
                state.apply(|s| s.failed_to_process(range));
            }
            result?;
        }

        #[cfg(test)]
        loop_callback();

        let n = notify.notified();
        let s = shutdown.wait();
        futures::pin_mut!(n);
        futures::pin_mut!(s);
        let s = futures::future::select(n, s).await;
        if let futures::future::Either::Right(_) = s {
            return Ok(false)
        }
    }
}

fn get_header_range_buffered(
    range: RangeInclusive<u32>,
    params: Params,
    p2p: Arc<impl PeerToPeerPort + 'static>,
) -> impl Stream<Item = anyhow::Result<SourcePeer<SealedBlockHeader>>> {
    get_header_range(range, p2p)
        .buffered(params.max_get_header_requests)
        .into_scan_none_or_err()
        .scan_none_or_err()
}

fn get_header_range(
    range: RangeInclusive<u32>,
    p2p: Arc<impl PeerToPeerPort + 'static>,
) -> impl Stream<
    Item = impl Future<Output = anyhow::Result<Option<SourcePeer<SealedBlockHeader>>>>,
> {
    stream::iter(range).map(move |height| {
        let p2p = p2p.clone();
        let height: BlockHeight = height.into();
        async move {
            Ok(p2p
                .get_sealed_block_header(height)
                .await?
                .and_then(|header| {
                    validate_header_height(height, &header.data).then_some(header)
                }))
        }
    })
}

fn validate_header_height(
    expected_height: BlockHeight,
    header: &SealedBlockHeader,
) -> bool {
    header.entity.consensus.height == expected_height
}

trait StreamUtil: Sized {
    fn into_scan_none_or_err(self) -> ScanNoneErr<Self> {
        ScanNoneErr(self)
    }

    fn into_scan_err(self) -> ScanErr<Self> {
        ScanErr(self)
    }
}

impl<S> StreamUtil for S {}

struct ScanNoneErr<S>(S);
struct ScanErr<S>(S);

type Fut<R> = futures::future::Ready<Option<anyhow::Result<R>>>;

impl<S> ScanNoneErr<S> {
    fn scan_none_or_err<R>(
        self,
    ) -> Scan<S, bool, Fut<R>, impl FnMut(&mut bool, <S as Stream>::Item) -> Fut<R>>
    where
        S: Stream<Item = anyhow::Result<Option<R>>>,
    {
        self.0.scan(false, |err, result| {
            if *err {
                futures::future::ready(None)
            } else {
                *err = result.is_err();
                futures::future::ready(result.transpose())
            }
        })
    }
}

impl<S> ScanErr<S> {
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

// impl<S, R> Stream for ScanErr<S>
// where
//     S: Stream<Item = anyhow::Result<R>>,
// {
//     type Item = anyhow::Result<R>;

//     fn poll_next(
//         self: std::pin::Pin<&mut Self>,
//         cx: &mut std::task::Context<'_>,
//     ) -> std::task::Poll<Option<Self::Item>> {
//         std::pin::Pin::new(&mut self.0).poll_next(cx)
//     }
// }
