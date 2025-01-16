use crate::{
    registry::RegistrationsPerTable,
    VersionedBlockPayload,
};
use fuel_core_types::{
    blockchain::{
        block::PartialFuelBlock,
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
    fuel_crypto,
    fuel_tx::{
        Bytes32,
        CompressedTransaction,
        UniqueIdentifier,
    },
    fuel_types::{
        BlockHeight,
        ChainId,
    },
};

pub fn generate_tx_commitment(tx_ids: &[Bytes32]) -> Bytes32 {
    let mut hasher = fuel_crypto::Hasher::default();
    for tx_id in tx_ids {
        hasher.input(tx_id.as_ref());
    }
    hasher.digest()
}

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
    // A commitment to all transaction ids in the block
    pub tx_commitment: Bytes32,
}

impl CompressedBlockHeader {
    fn new(header: &BlockHeader, tx_ids: &[Bytes32]) -> Self {
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
            tx_commitment: generate_tx_commitment(tx_ids),
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

    fn validate_with(
        &self,
        partial_block: &PartialFuelBlock,
        chain_id: &ChainId,
    ) -> anyhow::Result<()> {
        let txs = partial_block
            .transactions
            .iter()
            .map(|tx| tx.id(chain_id))
            .collect::<Vec<_>>();
        let tx_commitment = generate_tx_commitment(&txs);
        let expected = self.header.tx_commitment;
        if tx_commitment != expected {
            anyhow::bail!(
                "Invalid tx commitment. got {tx_commitment}, expected {expected}"
            );
        }
        Ok(())
    }
}

impl CompressedBlockPayloadV1 {
    /// Create a new compressed block payload V1.
    #[allow(unused)]
    pub(crate) fn new(
        header: &BlockHeader,
        registrations: RegistrationsPerTable,
        transactions: Vec<CompressedTransaction>,
        tx_ids: &[Bytes32],
    ) -> Self {
        Self {
            header: CompressedBlockHeader::new(header, tx_ids),
            registrations,
            transactions,
        }
    }
}
