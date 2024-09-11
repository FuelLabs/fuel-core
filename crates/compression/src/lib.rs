mod eviction_policy;
pub mod ports;
mod tables;
pub mod services {
    pub mod compress;
    pub mod decompress;
}
mod context {
    pub mod compress;
    pub mod decompress;
    pub mod prepare;
}

pub use tables::RegistryKeyspace;

#[cfg(test)]
mod compression_tests;

use serde::{
    Deserialize,
    Serialize,
};

use fuel_core_types::{
    blockchain::{
        header::{
            ConsensusParametersVersion,
            StateTransitionBytecodeVersion,
        },
        primitives::DaBlockHeight,
    },
    fuel_tx::CompressedTransaction,
    fuel_types::{
        BlockHeight,
        Bytes32,
    },
    tai64::Tai64,
};
use tables::RegistrationsPerTable;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Header {
    pub da_height: DaBlockHeight,
    pub prev_root: Bytes32,
    pub height: BlockHeight,
    pub time: Tai64,
    pub consensus_parameters_version: ConsensusParametersVersion,
    pub state_transition_bytecode_version: StateTransitionBytecodeVersion,
}

/// Compressed block, without the preceding version byte.
#[derive(Clone, Serialize, Deserialize)]
struct CompressedBlockPayload {
    /// Registration section of the compressed block
    registrations: RegistrationsPerTable,
    /// Compressed block header
    header: Header,
    /// Compressed transactions
    transactions: Vec<CompressedTransaction>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn postcard_roundtrip() {
        let original = CompressedBlockPayload {
            registrations: RegistrationsPerTable::default(),
            header: Header {
                da_height: DaBlockHeight::default(),
                prev_root: Default::default(),
                height: 3u32.into(),
                consensus_parameters_version: 1,
                state_transition_bytecode_version: 2,
                time: Tai64::UNIX_EPOCH,
            },
            transactions: vec![],
        };

        let compressed = postcard::to_allocvec(&original).unwrap();
        let decompressed: CompressedBlockPayload =
            postcard::from_bytes(&compressed).unwrap();

        let CompressedBlockPayload {
            registrations,
            header,
            transactions,
        } = decompressed;

        assert!(registrations.is_empty());
        assert_eq!(header.height, 3u32.into());
        assert!(transactions.is_empty());
    }
}
