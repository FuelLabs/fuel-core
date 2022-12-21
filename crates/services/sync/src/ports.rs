use std::ops::Range;

use async_trait::async_trait;
use fuel_core_types::{
    blockchain::{
        primitives::BlockHeight,
        SealedBlock,
        SealedBlockHeader,
    },
    services::p2p::{
        GossipsubMessageAcceptance,
        NetworkData,
    },
};

#[async_trait]
trait PeerToPeer {
    type SealedHeaderResponse: NetworkData<SealedBlockHeader>;
    type BlockResponse: NetworkData<Vec<SealedBlock>>;
    type GossipedBlockHeader: NetworkData<SealedBlockHeader>;

    async fn fetch_best_network_block_header(
    ) -> anyhow::Result<Option<Self::SealedHeaderResponse>>;

    async fn fetch_blocks(
        query: Range<BlockHeight>,
    ) -> anyhow::Result<Self::BlockResponse>;

    // punish the sender for providing an invalid block header
    fn report_invalid_block_header(
        invalid_header: &Self::SealedHeaderResponse,
    ) -> anyhow::Result<()>;

    // punish the sender for providing a set of blocks that aren't valid
    fn report_invalid_blocks(invalid_blocks: &Self::BlockResponse) -> anyhow::Result<()>;

    // await a newly produced block from the network (similar to stream.next())
    async fn next_gossiped_block_header() -> anyhow::Result<Self::SealedHeaderResponse>;

    // notify the p2p network whether to continue gossiping this message to others or
    // punish the peer that sent it
    fn notify_gossip_block_validity(
        message: &Self::GossipedBlockHeader,
        validity: GossipsubMessageAcceptance,
    );
}

#[async_trait]
trait BlockImporter {
    // commit a sealed block to the chain
    async fn commit(block: SealedBlock) -> anyhow::Result<()>;
}

#[async_trait]
trait Consensus {
    // check with the consensus layer whether a block header passes consensus rules
    async fn validate_sealed_block_header(block: SealedBlockHeader)
        -> anyhow::Result<()>;
}

trait Database {
    // get current fuel blockchain height
    fn get_current_height() -> anyhow::Result<BlockHeight>;
}
