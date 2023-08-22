//! Ports this services requires to function.

use fuel_core_services::stream::BoxStream;
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
    services::p2p::SourcePeer,
};
use std::ops::Range;

#[cfg_attr(test, mockall::automock)]
#[async_trait::async_trait]
/// Port for communication with the network.
pub trait PeerToPeerPort {
    /// Stream of newly observed block heights.
    fn height_stream(&self) -> BoxStream<BlockHeight>;

    /// Request a range of sealed block headers from the network.
    async fn get_sealed_block_headers(
        &self,
        block_height_range: Range<u32>,
    ) -> anyhow::Result<Option<Vec<SourcePeer<SealedBlockHeader>>>>;

    /// Request transactions from the network for the given block
    /// and source peer.
    async fn get_transactions(
        &self,
        block_id: SourcePeer<BlockId>,
    ) -> anyhow::Result<Option<Vec<Transaction>>>;
}

#[cfg_attr(test, mockall::automock)]
#[async_trait::async_trait]
/// Port for communication with the consensus service.
pub trait ConsensusPort {
    /// Check if the given sealed block header is valid.
    fn check_sealed_header(&self, header: &SealedBlockHeader) -> anyhow::Result<bool>;
    /// await for this DA height to be sync'd.
    async fn await_da_height(&self, da_height: &DaBlockHeight) -> anyhow::Result<()>;
}

#[cfg_attr(test, mockall::automock)]
#[async_trait::async_trait]
/// Port for communication with the block importer.
pub trait BlockImporterPort {
    /// Stream of newly committed block heights.
    fn committed_height_stream(&self) -> BoxStream<BlockHeight>;

    /// Execute the given sealed block
    /// and commit it to the database.
    async fn execute_and_commit(&self, block: SealedBlock) -> anyhow::Result<()>;
}
