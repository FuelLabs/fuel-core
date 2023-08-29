use super::{
    BlockImporterAdapter,
    P2PAdapter,
    VerifierAdapter,
};
use fuel_core_services::stream::BoxStream;
use fuel_core_sync::ports::{
    BlockImporterPort,
    ConsensusPort,
    PeerReportReason,
    PeerToPeerPort,
};
use fuel_core_types::{
    blockchain::{
        primitives::{
            BlockId,
            DaBlockHeight,
        },
        SealedBlock,
        SealedBlockHeader,
    },
    fuel_tx::Transaction,
    fuel_types::BlockHeight,
    services::p2p::{
        peer_reputation::{
            AppScore,
            PeerReport,
        },
        PeerId,
        SourcePeer,
    },
};
use std::ops::Range;

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

    async fn get_sealed_block_headers(
        &self,
        block_range_height: Range<u32>,
    ) -> anyhow::Result<SourcePeer<Option<Vec<SealedBlockHeader>>>> {
        if let Some(service) = &self.service {
            let (peer_id, headers) =
                service.get_sealed_block_headers(block_range_height).await?;
            let sourced_headers = SourcePeer {
                peer_id: peer_id.into(),
                data: headers,
            };
            Ok(sourced_headers)
        } else {
            Err(anyhow::anyhow!("No P2P service available"))
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
            Err(anyhow::anyhow!("No P2P service available"))
        }
    }

    async fn report_peer(
        &self,
        peer: PeerId,
        report: PeerReportReason,
    ) -> anyhow::Result<()> {
        if let Some(service) = &self.service {
            let service_name = "Sync";
            let new_peer_report_reason: NewPeerReportReason = report.into();
            service.report_peer(peer, new_peer_report_reason, service_name)?;
            Ok(())
        } else {
            Err(anyhow::anyhow!("No P2P service available"))
        }
    }
}

struct NewPeerReportReason(PeerReportReason);

impl From<PeerReportReason> for NewPeerReportReason {
    fn from(reason: PeerReportReason) -> Self {
        Self(reason)
    }
}

impl PeerReport for NewPeerReportReason {
    fn get_score_from_report(&self) -> AppScore {
        match self.0 {
            // Good
            PeerReportReason::SuccessfulBlockImport => 5.,
            // Bad
            PeerReportReason::BadBlockHeader => -100.,
            PeerReportReason::MissingTransactions => -100.,
            PeerReportReason::InvalidTransactions => -100.,
            PeerReportReason::MissingBlockHeaders => -100.,
            PeerReportReason::InvalidBlock => -100.,
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
