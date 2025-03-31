#![deny(clippy::arithmetic_side_effects)]
#![deny(clippy::cast_possible_truncation)]
#![deny(unused_crate_dependencies)]
#![deny(warnings)]

pub mod compress;
mod compressed_block_payload;
pub mod config;
pub mod decompress;
mod eviction_policy;
pub mod ports;
mod registry;

pub use config::Config;
use enum_dispatch::enum_dispatch;
pub use registry::RegistryKeyspace;

use crate::compressed_block_payload::v0::CompressedBlockPayloadV0;
#[cfg(feature = "fault-proving")]
use crate::compressed_block_payload::v1::CompressedBlockPayloadV1;
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
use registry::RegistrationsPerTable;

/// A compressed block payload MUST implement this trait
/// It is used to provide a convenient interface for usage within
/// compression
#[enum_dispatch]
pub trait VersionedBlockPayload {
    fn height(&self) -> &BlockHeight;
    fn consensus_header(&self) -> &ConsensusHeader<Empty>;
    fn application_header(&self) -> &ApplicationHeader<Empty>;
    fn registrations(&self) -> &RegistrationsPerTable;
    fn transactions(&self) -> Vec<CompressedTransaction>;
    fn partial_block_header(&self) -> PartialBlockHeader;
}

/// Versioned compressed block.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[enum_dispatch(VersionedBlockPayload)]
pub enum VersionedCompressedBlock {
    V0(CompressedBlockPayloadV0),
    #[cfg(feature = "fault-proving")]
    V1(CompressedBlockPayloadV1),
}

impl VersionedCompressedBlock {
    fn new(
        header: &BlockHeader,
        registrations: RegistrationsPerTable,
        transactions: Vec<CompressedTransaction>,
        #[cfg(feature = "fault-proving")]
        registry_root: crate::compressed_block_payload::v1::RegistryRoot,
    ) -> Self {
        #[cfg(not(feature = "fault-proving"))]
        return Self::V0(CompressedBlockPayloadV0::new(
            header,
            registrations,
            transactions,
        ));
        #[cfg(feature = "fault-proving")]
        Self::V1(CompressedBlockPayloadV1::new(
            header,
            registrations,
            transactions,
            registry_root,
        ))
    }
}

impl Default for VersionedCompressedBlock {
    fn default() -> Self {
        Self::V0(Default::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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

    fn keyspace() -> impl Strategy<Value = RegistryKeyspace> {
        prop_oneof![
            Just(RegistryKeyspace::Address),
            Just(RegistryKeyspace::AssetId),
            Just(RegistryKeyspace::ContractId),
            Just(RegistryKeyspace::ScriptCode),
            Just(RegistryKeyspace::PredicateCode),
        ]
    }

    #[derive(Debug)]
    struct PostcardRoundtripStrategy {
        da_height: u64,
        prev_root: [u8; 32],
        height: u32,
        consensus_parameters_version: u32,
        state_transition_bytecode_version: u32,
        registrations: RegistrationsPerTable,
    }

    fn postcard_roundtrip_strategy() -> impl Strategy<Value = PostcardRoundtripStrategy> {
        (
            0..=u64::MAX,
            prop::array::uniform32(0..=u8::MAX),
            0..=u32::MAX,
            0..=u32::MAX,
            0..=u32::MAX,
            prop::collection::vec(
                (
                    keyspace(),
                    prop::num::u16::ANY,
                    prop::array::uniform32(0..=u8::MAX),
                )
                    .prop_map(|(ks, rk, arr)| {
                        let k = RegistryKey::try_from(rk as u32).unwrap();
                        (ks, k, arr)
                    }),
                0..123,
            ),
        )
            .prop_map(
                |(
                    da_height,
                    prev_root,
                    height,
                    consensus_parameters_version,
                    state_transition_bytecode_version,
                    registration_inputs,
                )| {
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
                                registrations
                                    .script_code
                                    .push((key, arr[..value_len_limit].to_vec().into()));
                            }
                            RegistryKeyspace::PredicateCode => {
                                registrations
                                    .predicate_code
                                    .push((key, arr[..value_len_limit].to_vec().into()));
                            }
                        }
                    }

                    PostcardRoundtripStrategy {
                        da_height,
                        prev_root,
                        height,
                        consensus_parameters_version,
                        state_transition_bytecode_version,
                        registrations,
                    }
                },
            )
    }

    /// Serialization for compressed transactions is already tested in fuel-vm,
    /// but the rest of the block de/serialization is tested here.
    #[test]
    fn postcard_roundtrip_v0() {
        proptest!(|(strategy in postcard_roundtrip_strategy())| {
            let PostcardRoundtripStrategy {
                da_height,
                prev_root,
                height,
                consensus_parameters_version,
                state_transition_bytecode_version,
                registrations,
            } = strategy;

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

            let original = VersionedCompressedBlock::V0(CompressedBlockPayloadV0 {
                registrations,
                header,
                transactions: vec![],
            });

            let compressed = postcard::to_allocvec(&original).unwrap();
            let decompressed: VersionedCompressedBlock =
                postcard::from_bytes(&compressed).unwrap();

            let consensus_header = decompressed.consensus_header();
            let application_header = decompressed.application_header();

            assert_eq!(decompressed.registrations(), original.registrations());

            assert_eq!(application_header.da_height, da_height.into());
            assert_eq!(consensus_header.prev_root, prev_root.into());
            assert_eq!(consensus_header.height, height.into());
            assert_eq!(application_header.consensus_parameters_version, consensus_parameters_version);
            assert_eq!(application_header.state_transition_bytecode_version, state_transition_bytecode_version);

            assert!(decompressed.transactions().is_empty());
        });
    }

    #[cfg(feature = "fault-proving")]
    #[test]
    fn postcard_roundtrip_v1() {
        use compressed_block_payload::v1::{
            CompressedBlockHeader,
            CompressedBlockPayloadV1,
        };
        use fuel_core_types::blockchain::primitives::BlockId;
        use std::str::FromStr;

        use crate::compressed_block_payload::v1::RegistryRoot;

        proptest!(|(strategy in postcard_roundtrip_strategy())| {
            let PostcardRoundtripStrategy {
                da_height,
                prev_root,
                height,
                consensus_parameters_version,
                state_transition_bytecode_version,
                registrations,
            } = strategy;

            let header = CompressedBlockHeader {
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
                    generated: Empty,
                },
                block_id: BlockId::from_str("0xecea85c17070bc2e65f911310dbd01198f4436052ebba96cded9ddf30c58dd1a").unwrap(),
                registry_root: RegistryRoot::from_str("0xecea85c17070bc2e65f911310dbd01198f4436052ebba96cded9ddf30c58dd1b").unwrap(),
            };


            let original = VersionedCompressedBlock::V1(CompressedBlockPayloadV1 {
                header,
                registrations,
                transactions: vec![]
            });

            let compressed = postcard::to_allocvec(&original).unwrap();
            let decompressed: VersionedCompressedBlock =
                postcard::from_bytes(&compressed).unwrap();

            let consensus_header = decompressed.consensus_header();
            let application_header = decompressed.application_header();

            assert_eq!(decompressed.registrations(), original.registrations());

            assert_eq!(application_header.da_height, da_height.into());
            assert_eq!(consensus_header.prev_root, prev_root.into());
            assert_eq!(consensus_header.height, height.into());
            assert_eq!(application_header.consensus_parameters_version, consensus_parameters_version);
            assert_eq!(application_header.state_transition_bytecode_version, state_transition_bytecode_version);

            assert!(decompressed.transactions().is_empty());

            if let VersionedCompressedBlock::V1(block) = decompressed {
                assert_eq!(block.header.block_id, header.block_id);
                assert_eq!(block.header.registry_root, header.registry_root);
            } else {
                panic!("Expected V1 block, got {:?}", decompressed);
            }
        });
    }
}
