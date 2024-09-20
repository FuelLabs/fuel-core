#![deny(clippy::arithmetic_side_effects)]
#![deny(clippy::cast_possible_truncation)]
#![deny(unused_crate_dependencies)]
#![deny(warnings)]

pub mod compress;
pub mod decompress;
mod eviction_policy;
pub mod ports;
mod tables;
mod context {
    pub mod compress;
    pub mod decompress;
    pub mod prepare;
}

pub use tables::RegistryKeyspace;

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

/// Compressed block, without the preceding version byte.
#[derive(Clone, Serialize, Deserialize)]
struct CompressedBlockPayload {
    /// Temporal registry insertions
    registrations: RegistrationsPerTable,
    /// Merkle root of the temporal registry state
    registrations_root: Bytes32,
    /// Compressed block header
    header: Header,
    /// Compressed transactions
    transactions: Vec<CompressedTransaction>,
}

/// Fuel block header with only the fields required to reconstruct it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Header {
    da_height: DaBlockHeight,
    prev_root: Bytes32,
    height: BlockHeight,
    time: Tai64,
    consensus_parameters_version: ConsensusParametersVersion,
    state_transition_bytecode_version: StateTransitionBytecodeVersion,
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use fuel_core_types::{
        fuel_compression::RegistryKey,
        fuel_tx::{
            input::PredicateCode,
            Address,
            AssetId,
            ContractId,
            ScriptCode,
        },
    };
    use proptest::prelude::*;
    use tables::{
        PerRegistryKeyspace,
        PostcardSerialized,
    };

    use super::*;

    fn keyspace() -> impl Strategy<Value = RegistryKeyspace> {
        prop_oneof![
            Just(RegistryKeyspace::address),
            Just(RegistryKeyspace::asset_id),
            Just(RegistryKeyspace::contract_id),
            Just(RegistryKeyspace::script_code),
            Just(RegistryKeyspace::predicate_code),
        ]
    }

    fn keyspace_and_value(
    ) -> impl Strategy<Value = (RegistryKeyspace, PostcardSerialized)> {
        (keyspace(), prop::array::uniform32(0..u8::MAX)).prop_map(|(keyspace, value)| {
            let value = match keyspace {
                RegistryKeyspace::address => {
                    PostcardSerialized::new(Address::new(value)).unwrap()
                }
                RegistryKeyspace::asset_id => {
                    PostcardSerialized::new(AssetId::new(value)).unwrap()
                }
                RegistryKeyspace::contract_id => {
                    PostcardSerialized::new(ContractId::new(value)).unwrap()
                }
                RegistryKeyspace::script_code => {
                    let len = (value[0] % 32) as usize;
                    PostcardSerialized::new(ScriptCode {
                        bytes: value[..len].to_vec(),
                    })
                    .unwrap()
                }
                RegistryKeyspace::predicate_code => {
                    let len = (value[0] % 32) as usize;
                    PostcardSerialized::new(PredicateCode {
                        bytes: value[..len].to_vec(),
                    })
                    .unwrap()
                }
            };
            (keyspace, value)
        })
    }

    proptest! {
        /// Serialization for compressed transactions is already tested in fuel-vm,
        /// but the rest of the block de/serialization is be tested here.
        #[test]
        fn postcard_roundtrip(
            da_height in 0..u64::MAX,
            prev_root in prop::array::uniform32(0..u8::MAX),
            height in 0..u32::MAX,
            consensus_parameters_version in 0..u32::MAX,
            state_transition_bytecode_version in 0..u32::MAX,
            registrations_root in prop::array::uniform32(0..u8::MAX),
            registration_inputs in prop::collection::vec(
                (keyspace_and_value(), prop::num::u16::ANY).prop_map(|((ks, v), rk)| {
                    let k = RegistryKey::try_from(rk as u32).unwrap();
                    (ks, (k, v))
                }),
                0..123
            ),
        ) {
            let mut registrations: PerRegistryKeyspace<HashMap<RegistryKey, PostcardSerialized>> = Default::default();

            for (keyspace, (key, data)) in registration_inputs {
                registrations[keyspace].insert(key, data);
            }

            let original = CompressedBlockPayload {
                registrations: RegistrationsPerTable::try_from(registrations).unwrap(),
                registrations_root: registrations_root.into(),
                header: Header {
                    da_height: da_height.into(),
                    prev_root: prev_root.into(),
                    height: height.into(),
                    consensus_parameters_version,
                    state_transition_bytecode_version,
                    time: Tai64::UNIX_EPOCH,
                },
                transactions: vec![],
            };

            let compressed = postcard::to_allocvec(&original).unwrap();
            let decompressed: CompressedBlockPayload =
                postcard::from_bytes(&compressed).unwrap();

            let CompressedBlockPayload {
                registrations,
                registrations_root,
                header,
                transactions,
            } = decompressed;

            assert_eq!(registrations, original.registrations);
            assert_eq!(registrations_root, original.registrations_root);

            assert_eq!(header.da_height, da_height.into());
            assert_eq!(header.prev_root, prev_root.into());
            assert_eq!(header.height, height.into());
            assert_eq!(header.consensus_parameters_version, consensus_parameters_version);
            assert_eq!(header.state_transition_bytecode_version, state_transition_bytecode_version);

            assert!(transactions.is_empty());
        }
    }
}
