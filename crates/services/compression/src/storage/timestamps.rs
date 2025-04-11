//! The table that indexes the timestamps of the keys in the registry.

use fuel_core_types::{
    fuel_compression::RegistryKey,
    tai64::Tai64,
};

use fuel_core_storage::{
    Mappable,
    blueprint::plain::Plain,
    codec::postcard::Postcard,
    merkle::sparse::MerkleizedTableColumn,
    structured_storage::TableWithBlueprint,
};

use super::column::{
    CompressionColumn,
    MerkleizedColumnOf,
};

/// The metadata key used by `DaCompressionTemporalRegistryTimsetamps` table to
/// keep track of when each key was last updated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TimestampKey {
    /// The column where the key is stored.
    pub keyspace: TimestampKeyspace,
    /// The key itself.
    pub key: RegistryKey,
}

/// The keyspace for the timestamp key.
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
pub enum TimestampKeyspace {
    /// The address keyspace.
    Address,
    /// The asset ID keyspace.
    AssetId,
    /// The contract ID keyspace.
    ContractId,
    /// The script code keyspace.
    ScriptCode,
    /// The predicate code keyspace.
    PredicateCode,
}

#[cfg(feature = "test-helpers")]
impl rand::distributions::Distribution<TimestampKey> for rand::distributions::Standard {
    #![allow(clippy::arithmetic_side_effects)] // Test-only code, and also safe
    fn sample<R: rand::Rng + ?Sized>(&self, rng: &mut R) -> TimestampKey {
        TimestampKey {
            keyspace: rng.r#gen(),
            key: RegistryKey::try_from(rng.gen_range(0..2u32.pow(24) - 2)).unwrap(),
        }
    }
}

#[cfg(feature = "test-helpers")]
impl rand::distributions::Distribution<TimestampKeyspace>
    for rand::distributions::Standard
{
    fn sample<R: rand::Rng + ?Sized>(&self, rng: &mut R) -> TimestampKeyspace {
        use strum::EnumCount;
        match rng.next_u32() as usize % TimestampKeyspace::COUNT {
            0 => TimestampKeyspace::Address,
            1 => TimestampKeyspace::AssetId,
            2 => TimestampKeyspace::ContractId,
            3 => TimestampKeyspace::ScriptCode,
            4 => TimestampKeyspace::PredicateCode,
            _ => unreachable!("New metadata key is added but not supported here"),
        }
    }
}

/// Table that indexes the timestamps.
pub struct Timestamps;

impl MerkleizedTableColumn for Timestamps {
    type TableColumn = CompressionColumn;

    fn table_column() -> Self::TableColumn {
        Self::TableColumn::Timestamps
    }
}

impl Mappable for Timestamps {
    type Key = Self::OwnedKey;
    type OwnedKey = TimestampKey;
    type Value = Self::OwnedValue;
    type OwnedValue = Tai64;
}

impl TableWithBlueprint for Timestamps {
    type Blueprint = Plain<Postcard, Postcard>;
    type Column = MerkleizedColumnOf<Self>;

    fn column() -> Self::Column {
        Self::Column::TableColumn(Self::table_column())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fuel_core_storage::basic_storage_tests!(
        Timestamps,
        TimestampKey {
            keyspace: TimestampKeyspace::Address,
            key: RegistryKey::ZERO
        },
        Tai64::UNIX_EPOCH
    );
}
