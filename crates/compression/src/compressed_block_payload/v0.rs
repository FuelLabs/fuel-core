use crate::{
    VersionedBlockPayload,
    registry::RegistrationsPerTable,
};
use fuel_core_types::{
    blockchain::{
        header::{
            ApplicationHeader,
            BlockHeader,
            ConsensusHeader,
            PartialBlockHeader,
        },
        primitives::Empty,
    },
    fuel_tx::CompressedTransaction,
    fuel_types::BlockHeight,
};

/// Compressed block, without the preceding version byte.
#[derive(Debug, Default, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CompressedBlockPayloadV0 {
    /// Temporal registry insertions
    pub registrations: RegistrationsPerTable,
    /// Compressed block header
    pub header: PartialBlockHeader,
    /// Compressed transactions
    pub transactions: Vec<CompressedTransaction>,
}

impl VersionedBlockPayload for CompressedBlockPayloadV0 {
    fn height(&self) -> &BlockHeight {
        self.header.height()
    }

    fn consensus_header(&self) -> &ConsensusHeader<Empty> {
        &self.header.consensus
    }

    fn application_header(&self) -> &ApplicationHeader<Empty> {
        &self.header.application
    }

    fn registrations(&self) -> &RegistrationsPerTable {
        &self.registrations
    }

    fn transactions(&self) -> Vec<CompressedTransaction> {
        self.transactions.clone()
    }

    fn partial_block_header(&self) -> PartialBlockHeader {
        self.header
    }
}

impl CompressedBlockPayloadV0 {
    /// Create a new compressed block payload V0.
    #[allow(unused)]
    pub(crate) fn new(
        header: &BlockHeader,
        registrations: RegistrationsPerTable,
        transactions: Vec<CompressedTransaction>,
    ) -> Self {
        Self {
            header: PartialBlockHeader::from(header),
            registrations,
            transactions,
        }
    }
}
