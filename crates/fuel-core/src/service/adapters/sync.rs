use super::{
    BlockImporterAdapter,
    P2PAdapter,
    VerifierAdapter,
};
use fuel_core_services::stream::BoxStream;
use fuel_core_sync::ports::{
    BlockImporterPort,
    ConsensusPort,
    PeerToPeerPort,
};
use fuel_core_types::{
    blockchain::{
        primitives::{
            BlockHeight,
            BlockId,
            DaBlockHeight,
        },
        SealedBlock,
        SealedBlockHeader,
    },
    fuel_tx::Transaction,
    services::p2p::SourcePeer,
};

#[async_trait::async_trait]
impl PeerToPeerPort for P2PAdapter {
    fn height_stream(&self) -> BoxStream<BlockHeight> {
        use futures::StreamExt;
        if let Some(service) = &self.service {
            fuel_core_services::stream::IntoBoxStream::into_boxed(
                tokio_stream::wrappers::BroadcastStream::new(
                    service.subscribe_block_height(),
                )
                .filter_map(|r| futures::future::ready(r.ok().map(|r| r.block_height))),
            )
        } else {
            fuel_core_services::stream::IntoBoxStream::into_boxed(tokio_stream::pending())
        }
    }

    async fn get_sealed_block_header(
        &self,
        height: BlockHeight,
    ) -> anyhow::Result<Option<SourcePeer<SealedBlockHeader>>> {
        if let Some(service) = &self.service {
            Ok(service
                .get_sealed_block_header(height)
                .await?
                .map(|(peer_id, header)| SourcePeer {
                    peer_id: peer_id.into(),
                    data: header,
                }))
        } else {
            Ok(None)
        }
    }

    async fn get_transactions(
        &self,
        block: SourcePeer<BlockId>,
    ) -> anyhow::Result<Option<Vec<Transaction>>> {
        let SourcePeer {
            peer_id,
            data: block,
        } = block;
        if let Some(service) = &self.service {
            service
                .get_transactions_from_peer(peer_id.into(), block)
                .await
        } else {
            Ok(None)
        }
    }
}

#[async_trait::async_trait]
impl BlockImporterPort for BlockImporterAdapter {
    fn committed_height_stream(&self) -> BoxStream<BlockHeight> {
        use futures::StreamExt;
        fuel_core_services::stream::IntoBoxStream::into_boxed(
            tokio_stream::wrappers::BroadcastStream::new(self.block_importer.subscribe())
                .filter_map(|r| {
                    futures::future::ready(
                        r.ok().map(|r| *r.sealed_block.entity.header().height()),
                    )
                }),
        )
    }
    async fn execute_and_commit(&self, block: SealedBlock) -> anyhow::Result<()> {
        self.execute_and_commit(block).await
    }
}

#[async_trait::async_trait]
impl ConsensusPort for VerifierAdapter {
    fn check_sealed_header(&self, header: &SealedBlockHeader) -> anyhow::Result<bool> {
        Ok(self.block_verifier.verify_consensus(header))
    }
    async fn await_da_height(&self, da_height: &DaBlockHeight) -> anyhow::Result<()> {
        self.block_verifier.await_da_height(da_height).await
    }
}
