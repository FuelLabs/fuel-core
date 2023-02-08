//! Ports this services requires to function.

use fuel_core_services::stream::BoxStream;
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

#[cfg_attr(test, mockall::automock)]
#[async_trait::async_trait]
/// Port for communication with the network.
pub trait PeerToPeerPort {
    /// Stream of newly observed block heights.
    fn height_stream(&self) -> BoxStream<BlockHeight>;

    /// Request sealed block header from the network
    /// at the given height.
    ///
    /// Returns the source peer this header was received from.
    async fn get_sealed_block_header(
        &self,
        height: BlockHeight,
    ) -> anyhow::Result<Option<SourcePeer<SealedBlockHeader>>>;

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
