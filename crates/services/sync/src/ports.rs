//! Ports this services requires to function.

use fuel_core_services::stream::BoxStream;
use fuel_core_types::{
    blockchain::{
        primitives::DaBlockHeight,
        SealedBlock,
        SealedBlockHeader,
    },
    fuel_types::BlockHeight,
    services::p2p::{
        PeerId,
        SourcePeer,
        Transactions,
    },
};
use std::ops::Range;

/// Possible reasons to report a peer
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PeerReportReason {
    // Good
    /// Successfully imported block
    SuccessfulBlockImport,

    // Bad
    /// Did not receive advertised block headers
    MissingBlockHeaders,
    /// Report a peer for sending a bad block header
    BadBlockHeader,
    /// Did not receive advertised transactions
    MissingTransactions,
    /// Received invalid transactions
    InvalidTransactions,
}

#[async_trait::async_trait]
#[cfg_attr(any(test, feature = "benchmarking"), mockall::automock)]
/// Port for communication with the network.
pub trait PeerToPeerPort {
    /// Stream of newly observed block heights.
    fn height_stream(&self) -> BoxStream<BlockHeight>;

    /// Request a range of sealed block headers from the network.
    async fn get_sealed_block_headers(
        &self,
        block_height_range: Range<u32>,
    ) -> anyhow::Result<SourcePeer<Option<Vec<SealedBlockHeader>>>>;

    /// Request transactions from the network for the given block range
    async fn get_transactions(
        &self,
        block_ids: Range<u32>,
    ) -> anyhow::Result<SourcePeer<Option<Vec<Transactions>>>>;

    /// Request transactions from the network for the given block range
    /// and source peer.
    async fn get_transactions_from_peer(
        &self,
        block_ids: SourcePeer<Range<u32>>,
    ) -> anyhow::Result<Option<Vec<Transactions>>>;

    /// Report a peer for some reason to modify their reputation.
    fn report_peer(&self, peer: PeerId, report: PeerReportReason) -> anyhow::Result<()>;
}

#[cfg_attr(any(test, feature = "benchmarking"), mockall::automock)]
#[async_trait::async_trait]
/// Port for communication with the consensus service.
pub trait ConsensusPort {
    /// Check if the given sealed block header is valid.
    fn check_sealed_header(&self, header: &SealedBlockHeader) -> anyhow::Result<bool>;
    /// await for this DA height to be sync'd.
    async fn await_da_height(&self, da_height: &DaBlockHeight) -> anyhow::Result<()>;
}

#[cfg_attr(any(test, feature = "benchmarking"), mockall::automock)]
#[async_trait::async_trait]
/// Port for communication with the block importer.
pub trait BlockImporterPort {
    /// Stream of newly committed block heights.
    fn committed_height_stream(&self) -> BoxStream<BlockHeight>;

    /// Execute the given sealed block
    /// and commit it to the database.
    async fn execute_and_commit(&self, block: SealedBlock) -> anyhow::Result<()>;
}
