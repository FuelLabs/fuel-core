#![deny(clippy::arithmetic_side_effects)]
#![deny(clippy::cast_possible_truncation)]
#![deny(unused_crate_dependencies)]
#![deny(warnings)]

pub mod compress;
pub mod config;
pub mod decompress;
mod eviction_policy;
pub mod ports;
mod registry;

pub use config::Config;
pub use registry::RegistryKeyspace;

use fuel_core_types::{
    blockchain::header::PartialBlockHeader,
    fuel_tx::CompressedTransaction,
    fuel_types::BlockHeight,
};
use registry::RegistrationsPerTable;

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

/// Versioned compressed block.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum VersionedCompressedBlock {
    V0(CompressedBlockPayloadV0),
}

impl Default for VersionedCompressedBlock {
    fn default() -> Self {
        Self::V0(Default::default())
    }
}

impl VersionedCompressedBlock {
    /// Returns the height of the compressed block.
    pub fn height(&self) -> &BlockHeight {
        match self {
            VersionedCompressedBlock::V0(block) => block.header.height(),
        }
    }
}

#[cfg(test)]
mod tests {
    use fuel_core_compression as _;
    use fuel_core_types::{
        blockchain::{
            header::{
                ApplicationHeader,
                ConsensusHeader,
            },
            primitives::Empty,
        },
        fuel_compression::RegistryKey,
        tai64::Tai64,
    };
    use proptest::prelude::*;

    use super::*;

    fn keyspace() -> impl Strategy<Value = RegistryKeyspace> {
        prop_oneof![
            Just(RegistryKeyspace::Address),
            Just(RegistryKeyspace::AssetId),
            Just(RegistryKeyspace::ContractId),
            Just(RegistryKeyspace::ScriptCode),
            Just(RegistryKeyspace::PredicateCode),
        ]
    }

    proptest! {
        /// Serialization for compressed transactions is already tested in fuel-vm,
        /// but the rest of the block de/serialization is tested here.
        #[test]
        fn postcard_roundtrip(
            da_height in 0..=u64::MAX,
            prev_root in prop::array::uniform32(0..=u8::MAX),
            height in 0..=u32::MAX,
            consensus_parameters_version in 0..=u32::MAX,
            state_transition_bytecode_version in 0..=u32::MAX,
            registration_inputs in prop::collection::vec(
                (keyspace(), prop::num::u16::ANY, prop::array::uniform32(0..=u8::MAX)).prop_map(|(ks, rk, arr)| {
                    let k = RegistryKey::try_from(rk as u32).unwrap();
                    (ks, k, arr)
                }),
                0..123
            ),
        ) {
            let mut registrations: RegistrationsPerTable = Default::default();

            for (ks, key, arr) in registration_inputs {
                let value_len_limit = (key.as_u32() % 32) as usize;
                match ks {
                    RegistryKeyspace::Address => {
                        registrations.address.push((key, arr.into()));
                    }
                    RegistryKeyspace::AssetId => {
                        registrations.asset_id.push((key, arr.into()));
                    }
                    RegistryKeyspace::ContractId => {
                        registrations.contract_id.push((key, arr.into()));
                    }
                    RegistryKeyspace::ScriptCode => {
                        registrations.script_code.push((key, arr[..value_len_limit].to_vec().into()));
                    }
                    RegistryKeyspace::PredicateCode => {
                        registrations.predicate_code.push((key, arr[..value_len_limit].to_vec().into()));
                    }
                }
            }

            let header = PartialBlockHeader {
                application: ApplicationHeader {
                    da_height: da_height.into(),
                    consensus_parameters_version,
                    state_transition_bytecode_version,
                    generated: Empty,
                },
                consensus: ConsensusHeader {
                    prev_root: prev_root.into(),
                    height: height.into(),
                    time: Tai64::UNIX_EPOCH,
                    generated: Empty
                }
            };
            let original = CompressedBlockPayloadV0 {
                registrations,
                header,
                transactions: vec![],
            };

            let compressed = postcard::to_allocvec(&original).unwrap();
            let decompressed: CompressedBlockPayloadV0 =
                postcard::from_bytes(&compressed).unwrap();

            let CompressedBlockPayloadV0 {
                registrations,
                header,
                transactions,
            } = decompressed;

            assert_eq!(registrations, original.registrations);

            assert_eq!(header.da_height, da_height.into());
            assert_eq!(*header.prev_root(), prev_root.into());
            assert_eq!(*header.height(), height.into());
            assert_eq!(header.consensus_parameters_version, consensus_parameters_version);
            assert_eq!(header.state_transition_bytecode_version, state_transition_bytecode_version);

            assert!(transactions.is_empty());
        }
    }
}
