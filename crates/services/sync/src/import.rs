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
    stream,
    stream::StreamExt,
    Stream,
};
use std::future::Future;
use tokio::sync::Notify;

use crate::{
    ports::{
        DatabasePort,
        Executor,
        PeerToPeer,
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

pub(super) async fn import(
    state: SharedMutex<State>,
    notify: Arc<Notify>,
    params: Params,
    p2p: Arc<impl PeerToPeer + 'static>,
    executor: Arc<impl Executor + 'static>,
    shutdown: Shutdown,
) {
    loop {
        if let Some(range) = state.apply(|s| s.process_range()) {
            let count = get_header_range_buffered(range.clone(), params, p2p.clone())
                .map(|header| {
                    let SourcePeer {
                        peer_id,
                        data:
                            Sealed {
                                entity: header,
                                consensus,
                            },
                    } = header;
                    let id = header.id();
                    let block_id = SourcePeer { peer_id, data: id };
                    let p2p = p2p.clone();
                    async move {
                        p2p.get_transactions(block_id).await.unwrap().and_then(
                            |transactions| {
                                Some(SealedBlock {
                                    entity: Block::try_from_executed(
                                        header,
                                        transactions,
                                    )?,
                                    consensus,
                                })
                            },
                        )
                    }
                })
                .buffered(params.max_get_txns_requests)
                .scan((), |_, block| futures::future::ready(block))
                .take_until({
                    let s = shutdown.clone();
                    async move { s.wait().await }
                })
                .then(|block| {
                    let state = state.clone();
                    let executor = executor.clone();
                    async move {
                        let height = *block.entity.header().height();
                        let r = executor.execute_and_commit(block).await.is_ok();
                        if r {
                            state.apply(|s| s.commit(*height))
                        }
                        r
                    }
                })
                .take_while(|r| futures::future::ready(*r))
                .count()
                .await;

            let range_len = range.end().saturating_sub(*range.start());
            if (count as u32) < range_len {
                let range = (*range.end() - count as u32)..=*range.end();
                state.apply(|s| s.failed_to_process(range));
            }
        }
        let n = notify.notified();
        let s = shutdown.wait();
        futures::pin_mut!(n);
        futures::pin_mut!(s);
        let s = futures::future::select(n, s).await;
        if let futures::future::Either::Right(_) = s {
            return
        }
    }
}

fn get_header_range_buffered(
    range: RangeInclusive<u32>,
    params: Params,
    p2p: Arc<impl PeerToPeer + 'static>,
) -> impl Stream<Item = SourcePeer<SealedBlockHeader>> {
    get_header_range(range, p2p)
        .buffered(params.max_get_header_requests)
        .scan((), |_, h| futures::future::ready(h))
}

fn get_header_range(
    range: RangeInclusive<u32>,
    p2p: Arc<impl PeerToPeer + 'static>,
) -> impl Stream<Item = impl Future<Output = Option<SourcePeer<SealedBlockHeader>>>> {
    stream::iter(range).map(move |height| {
        let p2p = p2p.clone();
        let height: BlockHeight = height.into();
        async move {
            let header = p2p.get_sealed_block_header(height).await.ok().unwrap()?;
            validate_header_height(height, &header.data).then_some(header)
        }
    })
}

fn validate_header_height(
    expected_height: BlockHeight,
    header: &SealedBlockHeader,
) -> bool {
    header.entity.consensus.height == expected_height
}
