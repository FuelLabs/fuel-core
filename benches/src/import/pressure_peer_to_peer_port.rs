use fuel_core_services::stream::BoxStream;
use fuel_core_sync::ports::{
    MockPeerToPeerPort,
    PeerToPeerPort,
};
use fuel_core_types::{
    blockchain::{
        consensus::{
            Consensus,
            Sealed,
        },
        header::BlockHeader,
        primitives::BlockId,
        SealedBlockHeader,
    },
    fuel_tx::Transaction,
    fuel_types::BlockHeight,
    services::p2p::SourcePeer,
};
use std::time::Duration;

fn empty_header(h: BlockHeight) -> SourcePeer<SealedBlockHeader> {
    let mut header = BlockHeader::default();
    header.consensus.height = h;
    let transaction_tree =
        fuel_core_types::fuel_merkle::binary::in_memory::MerkleTree::new();
    header.application.generated.transactions_root = transaction_tree.root().into();

    let consensus = Consensus::default();
    let sealed = Sealed {
        entity: header,
        consensus,
    };
    SourcePeer {
        peer_id: vec![].into(),
        data: sealed,
    }
}

pub struct PressurePeerToPeerPort(MockPeerToPeerPort, [Duration; 2]);

impl PressurePeerToPeerPort {
    pub fn new(delays: [Duration; 2]) -> Self {
        let mut mock = MockPeerToPeerPort::default();
        mock.expect_get_sealed_block_header()
            .returning(|h| Ok(Some(empty_header(h))));
        mock.expect_get_transactions()
            .returning(|_| Ok(Some(vec![])));
        Self(mock, delays)
    }

    fn service(&self) -> &impl PeerToPeerPort {
        &self.0
    }

    fn duration(&self, index: usize) -> Duration {
        self.1[index]
    }
}

#[async_trait::async_trait]
impl PeerToPeerPort for PressurePeerToPeerPort {
    fn height_stream(&self) -> BoxStream<BlockHeight> {
        self.service().height_stream()
    }

    async fn get_sealed_block_header(
        &self,
        height: BlockHeight,
    ) -> anyhow::Result<Option<SourcePeer<SealedBlockHeader>>> {
        let timeout = self.duration(0);
        tokio::time::sleep(timeout).await;
        self.service().get_sealed_block_header(height).await
    }

    async fn get_transactions(
        &self,
        block_id: SourcePeer<BlockId>,
    ) -> anyhow::Result<Option<Vec<Transaction>>> {
        let timeout = self.duration(1);
        tokio::time::sleep(timeout).await;
        self.service().get_transactions(block_id).await
    }
}
