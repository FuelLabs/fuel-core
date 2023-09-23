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
        Transactions,
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

    async fn select_peer(
        &self,
        block_height: BlockHeight,
    ) -> anyhow::Result<Option<PeerId>> {
        if let Some(service) = &self.service {
            let peer_id = service.select_peer(block_height).await?;
            Ok(peer_id)
        } else {
            Err(anyhow::anyhow!("No P2P service available"))
        }
    }

    async fn get_sealed_block_headers(
        &self,
        block_height_range: SourcePeer<Range<u32>>,
    ) -> SourcePeer<anyhow::Result<Option<Vec<SealedBlockHeader>>>> {
        let SourcePeer {
            peer_id,
            data: block_height_range,
        } = block_height_range;
        let result = if let Some(service) = &self.service {
            let headers = service
                .get_sealed_block_headers(peer_id.clone().into(), block_height_range)
                .await;
            headers
        } else {
            Err(anyhow::anyhow!("No P2P service available"))
        };
        peer_id.bind(result)
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

    async fn get_transactions_2(
        &self,
        block_ids: SourcePeer<Vec<BlockId>>,
    ) -> anyhow::Result<Option<Vec<Transactions>>> {
        let SourcePeer {
            peer_id,
            data: blocks,
        } = block_ids;
        if let Some(service) = &self.service {
            service
                .get_transactions_2_from_peer(peer_id.into(), blocks)
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
            let new_report = self.process_report(report);
            service.report_peer(peer, new_report, service_name)?;
            Ok(())
        } else {
            Err(anyhow::anyhow!("No P2P service available"))
        }
    }
}

impl P2PAdapter {
    fn process_report(&self, reason: PeerReportReason) -> P2PAdapterPeerReport {
        let score = match &reason {
            PeerReportReason::SuccessfulBlockImport => {
                self.peer_report_config.successful_block_import
            }
            PeerReportReason::MissingBlockHeaders => {
                self.peer_report_config.missing_block_headers
            }
            PeerReportReason::BadBlockHeader => self.peer_report_config.bad_block_header,
            PeerReportReason::MissingTransactions => {
                self.peer_report_config.missing_transactions
            }
            PeerReportReason::InvalidTransactions => {
                self.peer_report_config.invalid_transactions
            }
        };
        P2PAdapterPeerReport { score }
    }
}

struct P2PAdapterPeerReport {
    score: AppScore,
}

impl PeerReport for P2PAdapterPeerReport {
    fn get_score_from_report(&self) -> AppScore {
        self.score
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
