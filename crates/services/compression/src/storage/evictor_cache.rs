//! Evictor Cache table.

use fuel_core_storage::{
    Mappable,
    blueprint::plain::Plain,
    codec::postcard::Postcard,
    structured_storage::TableWithBlueprint,
};
use fuel_core_types::fuel_compression::RegistryKey;

use super::column::CompressionColumn;

/// The metadata key used by `EvictorCache` table to
/// store progress of the evictor.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    strum::EnumCount,
)]
pub enum MetadataKey {
    /// Address
    Address,
    /// Asset ID
    AssetId,
    /// Contract ID
    ContractId,
    /// Script code
    ScriptCode,
    /// Predicate code
    PredicateCode,
}

#[cfg(feature = "test-helpers")]
impl rand::distributions::Distribution<MetadataKey> for rand::distributions::Standard {
    fn sample<R: rand::Rng + ?Sized>(&self, rng: &mut R) -> MetadataKey {
        use strum::EnumCount;
        match rng.next_u32() as usize % MetadataKey::COUNT {
            0 => MetadataKey::Address,
            1 => MetadataKey::AssetId,
            2 => MetadataKey::ContractId,
            3 => MetadataKey::ScriptCode,
            4 => MetadataKey::PredicateCode,
            _ => unreachable!("New metadata key is added but not supported here"),
        }
    }
}

/// Table that indexes the addresses.
pub struct EvictorCache;

impl Mappable for EvictorCache {
    type Key = Self::OwnedKey;
    type OwnedKey = MetadataKey;
    type Value = Self::OwnedValue;
    type OwnedValue = RegistryKey;
}

impl TableWithBlueprint for EvictorCache {
    type Blueprint = Plain<Postcard, Postcard>;
    type Column = CompressionColumn;

    fn column() -> Self::Column {
        Self::Column::EvictorCache
    }
}

// no test for this table ~ not enough randomization for `MetadataKey`
