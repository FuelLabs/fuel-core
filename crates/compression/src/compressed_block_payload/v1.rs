use fuel_core_types::{
    blockchain::{
        header::{
            ApplicationHeader,
            ConsensusHeader,
            PartialBlockHeader,
        },
        primitives::Empty,
    },
    fuel_tx::CompressedTransaction,
    fuel_types::BlockHeight,
};

use crate::{
    registry::RegistrationsPerTable,
    CompressedBlockHeader,
    VersionedBlockPayload,
};

#[derive(Debug, Default, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CompressedBlockPayloadV1 {
    /// Temporal registry insertions
    pub registrations: RegistrationsPerTable,
    /// Compressed block header
    pub header: CompressedBlockHeader,
    /// Compressed transactions
    pub transactions: Vec<CompressedTransaction>,
}

impl VersionedBlockPayload for CompressedBlockPayloadV1 {
    fn height(&self) -> &BlockHeight {
        &self.header.consensus.height
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
        PartialBlockHeader::from(&self.header)
    }
}
