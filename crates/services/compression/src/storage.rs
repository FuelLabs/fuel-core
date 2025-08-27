use fuel_core_storage::merkle::sparse::Merkleized;

/// Generates a random RegistryKey
#[cfg(feature = "test-helpers")]
#[allow(clippy::arithmetic_side_effects)]
pub fn generate_key(
    rng: &mut impl rand::Rng,
) -> fuel_core_types::fuel_compression::RegistryKey {
    let raw_key: u32 = rng.gen_range(0..2u32.pow(24) - 2);
    fuel_core_types::fuel_compression::RegistryKey::try_from(raw_key).unwrap()
}

pub mod address;
pub mod asset_id;
pub mod column;
pub mod compressed_blocks;
pub mod contract_id;
pub mod evictor_cache;
pub mod predicate_code;
pub mod registry_index;
pub mod script_code;
// TODO: https://github.com/FuelLabs/fuel-core/issues/2842
#[cfg(feature = "fault-proving")]
pub mod registrations;
pub mod timestamps;

/// Merkleized Address table type alias
pub type Address = Merkleized<address::Address>;

/// Merkleized AssetId table type alias
pub type AssetId = Merkleized<asset_id::AssetId>;

/// Merkleized ContractId table type alias
pub type ContractId = Merkleized<contract_id::ContractId>;

/// Merkleized EvictorCache table type alias
pub type EvictorCache = Merkleized<evictor_cache::EvictorCache>;

/// Merkleized PredicateCode table type alias
pub type PredicateCode = Merkleized<predicate_code::PredicateCode>;

/// Merkleized ScriptCode table type alias
pub type ScriptCode = Merkleized<script_code::ScriptCode>;

/// Merkleized RegistryIndex table type alias
pub type RegistryIndex = Merkleized<registry_index::RegistryIndex>;

/// Merkleized Timestamps table type alias
pub type Timestamps = Merkleized<timestamps::Timestamps>;

/// Re-export to match api
pub use compressed_blocks::CompressedBlocks;

/// Merkleized Registrations table type alias
#[cfg(feature = "fault-proving")]
pub type Registrations = Merkleized<registrations::Registrations>;
