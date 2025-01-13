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

#[cfg(feature = "fault-proving")]
use fuel_core_types::blockchain::header::BlockHeader;
#[cfg(feature = "fault-proving")]
use fuel_core_types::blockchain::primitives::BlockId;
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

#[cfg(feature = "fault-proving")]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(
    feature = "fault-proving",
    derive(serde::Serialize, serde::Deserialize)
)]
#[derive(Default)]
/// A partially complete fuel block header that does not
/// have any generated fields because it has not been executed yet.
pub struct CompressedBlockHeader {
    /// The application header.
    pub application: ApplicationHeader<Empty>,
    /// The consensus header.
    pub consensus: ConsensusHeader<Empty>,
    // The block id.
    pub block_id: BlockId,
}

#[cfg(feature = "fault-proving")]
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

#[cfg(feature = "fault-proving")]
impl From<&CompressedBlockHeader> for PartialBlockHeader {
    fn from(value: &CompressedBlockHeader) -> Self {
        PartialBlockHeader {
            application: value.application,
            consensus: value.consensus,
        }
    }
}

#[cfg(feature = "fault-proving")]
#[derive(Debug, Default, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CompressedBlockPayloadV1 {
    /// Temporal registry insertions
    pub registrations: RegistrationsPerTable,
    /// Compressed block header
    pub header: CompressedBlockHeader,
    /// Compressed transactions
    pub transactions: Vec<CompressedTransaction>,
}

/// Versioned compressed block.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum VersionedCompressedBlock {
    V0(CompressedBlockPayloadV0),
    #[cfg(feature = "fault-proving")]
    V1(CompressedBlockPayloadV1),
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
            #[cfg(feature = "fault-proving")]
            VersionedCompressedBlock::V1(block) => &block.header.consensus.height,
        }
    }

    /// Returns the consensus header
    pub fn consensus_header(&self) -> &ConsensusHeader<Empty> {
        match self {
            VersionedCompressedBlock::V0(block) => &block.header.consensus,
            #[cfg(feature = "fault-proving")]
            VersionedCompressedBlock::V1(block) => &block.header.consensus,
        }
    }

    /// Returns the application header
    pub fn application_header(&self) -> &ApplicationHeader<Empty> {
        match self {
            VersionedCompressedBlock::V0(block) => &block.header.application,
            #[cfg(feature = "fault-proving")]
            VersionedCompressedBlock::V1(block) => &block.header.application,
        }
    }

    /// Returns the registrations table
    pub fn registrations(&self) -> &RegistrationsPerTable {
        match self {
            VersionedCompressedBlock::V0(block) => &block.registrations,
            #[cfg(feature = "fault-proving")]
            VersionedCompressedBlock::V1(block) => &block.registrations,
        }
    }

    /// Returns the transactions
    pub fn transactions(&self) -> Vec<CompressedTransaction> {
        match self {
            VersionedCompressedBlock::V0(block) => block.transactions.clone(),
            #[cfg(feature = "fault-proving")]
            VersionedCompressedBlock::V1(block) => block.transactions.clone(),
        }
    }

    /// Returns the partial block header
    pub fn partial_block_header(&self) -> PartialBlockHeader {
        match self {
            VersionedCompressedBlock::V0(block) => block.header,
            #[cfg(feature = "fault-proving")]
            VersionedCompressedBlock::V1(block) => {
                PartialBlockHeader::from(&block.header)
            }
        }
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
    #[cfg(feature = "fault-proving")]
    use std::str::FromStr;

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

            #[cfg(not(feature = "fault-proving"))]
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
            #[cfg(feature = "fault-proving")]
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
            };

            #[cfg(not(feature = "fault-proving"))]
            let original = VersionedCompressedBlock::V0(CompressedBlockPayloadV0 {
                registrations,
                header,
                transactions: vec![],
            });
            let original = VersionedCompressedBlock::V1(CompressedBlockPayloadV1 {
                registrations,
                header,
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
            }
        }
    }
}
