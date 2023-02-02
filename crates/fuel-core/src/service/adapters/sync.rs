use super::{
    BlockImporterAdapter,
    MaybeRelayerAdapter,
    P2PAdapter,
    VerifierAdapter,
};
use fuel_core_services::stream::BoxStream;
use fuel_core_sync::ports::{
    BlockImporterPort,
    ConsensusPort,
    PeerToPeerPort,
    RelayerPort,
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
        self.service
            .get_transactions_from_peer(peer_id.into(), block)
            .await
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

impl ConsensusPort for VerifierAdapter {
    fn check_sealed_header(&self, header: &SealedBlockHeader) -> anyhow::Result<bool> {
        Ok(self.block_verifier.verify_consensus(header))
    }
}

#[async_trait::async_trait]
impl RelayerPort for MaybeRelayerAdapter {
    async fn await_until_if_in_range(
        &self,
        da_height: &DaBlockHeight,
        max_da_lag: &DaBlockHeight,
    ) -> anyhow::Result<()> {
        #[cfg(feature = "relayer")]
        {
            use fuel_core_relayer::ports::RelayerDb;
            let current_height = self.database.get_finalized_da_height()?;
            anyhow::ensure!(
                current_height.abs_diff(**da_height) <= **max_da_lag,
                "Relayer is too far out of sync"
            );
            if let Some(sync) = self.relayer_synced.as_ref() {
                sync.await_at_least_synced(da_height).await?;
            }
            Ok(())
        }
        #[cfg(not(feature = "relayer"))]
        {
            core::mem::drop(max_da_lag);
            anyhow::ensure!(
                **da_height == 0,
                "Cannot have a da height above zero without a relayer"
            );
            Ok(())
        }
    }
}
