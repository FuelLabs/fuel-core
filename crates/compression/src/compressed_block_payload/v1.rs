use crate::{
    registry::RegistrationsPerTable,
    VersionedBlockPayload,
};
use fuel_core_types::{
    blockchain::{
        header::{
            ApplicationHeader,
            BlockHeader,
            ConsensusHeader,
            PartialBlockHeader,
        },
        primitives::{
            BlockId,
            Empty,
        },
    },
    fuel_tx::CompressedTransaction,
    fuel_types::BlockHeight,
};

/// A partially complete fuel block header that does not
/// have any generated fields because it has not been executed yet.
#[derive(
    Copy, Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize,
)]
pub struct CompressedBlockHeader {
    /// The application header.
    pub application: ApplicationHeader<Empty>,
    /// The consensus header.
    pub consensus: ConsensusHeader<Empty>,
    // The block id.
    pub block_id: BlockId,
}

impl From<&BlockHeader> for CompressedBlockHeader {
    fn from(header: &BlockHeader) -> Self {
        let ConsensusHeader {
            prev_root,
            height,
            time,
            ..
        } = *header.consensus();
        CompressedBlockHeader {
            application: ApplicationHeader {
                da_height: header.da_height,
                consensus_parameters_version: header.consensus_parameters_version,
                state_transition_bytecode_version: header
                    .state_transition_bytecode_version,
                generated: Empty {},
            },
            consensus: ConsensusHeader {
                prev_root,
                height,
                time,
                generated: Empty {},
            },
            block_id: header.id(),
        }
    }
}

impl From<&CompressedBlockHeader> for PartialBlockHeader {
    fn from(value: &CompressedBlockHeader) -> Self {
        PartialBlockHeader {
            application: value.application,
            consensus: value.consensus,
        }
    }
}

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

impl CompressedBlockPayloadV1 {
    /// Create a new compressed block payload V1.
    #[allow(unused)]
    pub(crate) fn new(
        header: &BlockHeader,
        registrations: RegistrationsPerTable,
        transactions: Vec<CompressedTransaction>,
    ) -> Self {
        Self {
            header: CompressedBlockHeader::from(header),
            registrations,
            transactions,
        }
    }
}
