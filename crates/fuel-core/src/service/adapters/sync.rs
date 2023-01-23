use super::P2PAdapter;
use fuel_core_services::stream::BoxStream;
use fuel_core_sync::ports::PeerToPeerPort;
use fuel_core_types::{
    blockchain::{
        primitives::{
            BlockHeight,
            BlockId,
        },
        SealedBlockHeader,
    },
    fuel_tx::Transaction,
    services::p2p::SourcePeer,
};

#[cfg(feature = "p2p")]
#[async_trait::async_trait]
impl PeerToPeerPort for P2PAdapter {
    fn height_stream(&self) -> BoxStream<BlockHeight> {
        use futures::StreamExt;
        fuel_core_services::stream::IntoBoxStream::into_boxed(
            tokio_stream::wrappers::BroadcastStream::new(
                self.service.subscribe_block_height(),
            )
            .filter_map(|r| futures::future::ready(r.ok().map(|r| r.block_height))),
        )
    }

    async fn get_sealed_block_header(
        &self,
        height: BlockHeight,
    ) -> anyhow::Result<Option<SourcePeer<SealedBlockHeader>>> {
        Ok(self.service.get_sealed_block_header(height).await?.map(
            |(peer_id, header)| SourcePeer {
                peer_id: peer_id.into(),
                data: header,
            },
        ))
    }

    async fn get_transactions(
        &self,
        block: SourcePeer<BlockId>,
    ) -> anyhow::Result<Option<Vec<Transaction>>> {
        let SourcePeer {
            peer_id,
            data: block,
        } = block;
        self.service.get_transactions_from_peer(peer_id.into(), block).await
            // TODO: The underlying api should return an option
            .map(Option::Some)
    }
}
