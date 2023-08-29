use crate::{
    import::test_helpers::{
        empty_header,
        SharedCounts,
    },
    ports::{
        MockPeerToPeerPort,
        PeerReportReason,
        PeerToPeerPort,
    },
};
use fuel_core_services::stream::BoxStream;
use fuel_core_types::{
    blockchain::{
        primitives::BlockId,
        SealedBlockHeader,
    },
    fuel_tx::Transaction,
    fuel_types::BlockHeight,
    services::p2p::{
        PeerId,
        SourcePeer,
    },
};
use std::{
    ops::Range,
    time::Duration,
};

pub struct PressurePeerToPeer {
    p2p: MockPeerToPeerPort,
    durations: [Duration; 2],
    counts: SharedCounts,
}

#[async_trait::async_trait]
impl PeerToPeerPort for PressurePeerToPeer {
    fn height_stream(&self) -> BoxStream<BlockHeight> {
        self.p2p.height_stream()
    }

    async fn get_sealed_block_headers(
        &self,
        block_height_range: Range<u32>,
    ) -> anyhow::Result<SourcePeer<Option<Vec<SealedBlockHeader>>>> {
        self.counts.apply(|c| c.inc_headers());
        tokio::time::sleep(self.durations[0]).await;
        self.counts.apply(|c| c.dec_headers());
        for _ in block_height_range.clone() {
            self.counts.apply(|c| c.inc_blocks());
        }
        self.p2p.get_sealed_block_headers(block_height_range).await
    }

    async fn get_transactions(
        &self,
        block_id: SourcePeer<BlockId>,
    ) -> anyhow::Result<Option<Vec<Transaction>>> {
        self.counts.apply(|c| c.inc_transactions());
        tokio::time::sleep(self.durations[1]).await;
        self.counts.apply(|c| c.dec_transactions());
        self.p2p.get_transactions(block_id).await
    }

    async fn report_peer(
        &self,
        _peer: PeerId,
        _report: PeerReportReason,
    ) -> anyhow::Result<()> {
        Ok(())
    }
}

impl PressurePeerToPeer {
    pub fn new(counts: SharedCounts, delays: [Duration; 2]) -> Self {
        let mut mock = MockPeerToPeerPort::default();
        mock.expect_get_sealed_block_headers().returning(|range| {
            let headers = Some(
                range
                    .clone()
                    .map(BlockHeight::from)
                    .map(empty_header)
                    .collect(),
            );
            let peer_id = vec![].into();
            let source_peer_data = SourcePeer {
                peer_id,
                data: headers,
            };
            Ok(source_peer_data)
        });
        mock.expect_get_transactions()
            .returning(|_| Ok(Some(vec![])));
        Self {
            p2p: mock,
            durations: delays,
            counts,
        }
    }
}
