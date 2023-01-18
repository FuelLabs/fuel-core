//! Ports this services requires to function.

use std::sync::Arc;

use fuel_core_services::stream::BoxStream;
use fuel_core_types::{
    blockchain::{
        primitives::{
            BlockHeight,
            BlockId,
        },
        SealedBlock,
        SealedBlockHeader,
    },
    fuel_tx::Transaction,
    services::{
        executor::ExecutionResult,
        p2p::SourcePeer,
    },
};

/// All ports this service requires to function.
pub struct Ports<P, E, C>
where
    P: PeerToPeerPort + 'static,
    E: BlockImporterPort + 'static,
    C: ConsensusPort + 'static,
{
    /// Network port.
    pub p2p: Arc<P>,
    /// Executor port.
    pub executor: Arc<E>,
    /// Consensus port.
    pub consensus: Arc<C>,
}

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
    async fn check_sealed_header(
        &self,
        header: &SealedBlockHeader,
    ) -> anyhow::Result<bool>;
}

#[cfg_attr(test, mockall::automock)]
#[async_trait::async_trait]
/// Port for communication with the block importer.
pub trait BlockImporterPort {
    /// Execute the given sealed block
    /// and commit it to the database.
    async fn execute_and_commit(
        &self,
        block: SealedBlock,
    ) -> anyhow::Result<ExecutionResult>;
}
