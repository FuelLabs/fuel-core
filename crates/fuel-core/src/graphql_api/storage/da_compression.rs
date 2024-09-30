use self::{
    evictor_cache::MetadataKey,
    predicate_code_codec::PredicateCodeCodec,
    reverse_key::ReverseKey,
    script_code_codec::ScriptCodeCodec,
    timestamps::TimestampKey,
};
use fuel_core_compression::VersionedCompressedBlock;
use fuel_core_storage::{
    blueprint::plain::Plain,
    codec::{
        postcard::Postcard,
        primitive::Primitive,
        raw::Raw,
    },
    structured_storage::TableWithBlueprint,
    Mappable,
};
use fuel_core_types::{
    fuel_compression::RegistryKey,
    fuel_tx::{
        input::PredicateCode,
        Address,
        AssetId,
        ContractId,
        ScriptCode,
    },
    fuel_types::BlockHeight,
    tai64::Tai64,
};

pub mod evictor_cache;
pub mod predicate_code_codec;
pub mod reverse_key;
pub mod script_code_codec;
pub mod timestamps;

/// The table for the compressed blocks sent to DA.
pub struct DaCompressedBlocks;

impl Mappable for DaCompressedBlocks {
    type Key = Self::OwnedKey;
    type OwnedKey = BlockHeight;
    type Value = Self::OwnedValue;
    type OwnedValue = VersionedCompressedBlock;
}

impl TableWithBlueprint for DaCompressedBlocks {
    type Blueprint = Plain<Primitive<4>, Postcard>;
    type Column = super::Column;

    fn column() -> Self::Column {
        Self::Column::DaCompressedBlocks
    }
}

/// Mapping from the type to the registry key in the temporal registry.
pub struct DaCompressionTemporalRegistryIndex;

impl Mappable for DaCompressionTemporalRegistryIndex {
    type Key = Self::OwnedKey;
    type OwnedKey = ReverseKey;
    type Value = Self::OwnedValue;
    type OwnedValue = RegistryKey;
}

impl TableWithBlueprint for DaCompressionTemporalRegistryIndex {
    // TODO: Use Raw codec for value instead of Postcard
    type Blueprint = Plain<Postcard, Postcard>;
    type Column = super::Column;

    fn column() -> Self::Column {
        Self::Column::DaCompressionTemporalRegistryIndex
    }
}

/// This table keeps track of last written timestamp for each key,
/// so that we can keep track of expiration.
pub struct DaCompressionTemporalRegistryTimestamps;

impl Mappable for DaCompressionTemporalRegistryTimestamps {
    type Key = Self::OwnedKey;
    type OwnedKey = TimestampKey;
    type Value = Self::OwnedValue;
    type OwnedValue = Tai64;
}

impl TableWithBlueprint for DaCompressionTemporalRegistryTimestamps {
    // TODO: Use Raw codec for value instead of Postcard
    type Blueprint = Plain<Postcard, Postcard>;
    type Column = super::Column;

    fn column() -> Self::Column {
        Self::Column::DaCompressionTemporalRegistryTimestamps
    }
}

/// This table is used to hold "next key to evict" for each keyspace.
/// In the future we'll likely switch to use LRU or something, in which
/// case this table can be repurposed.
pub struct DaCompressionTemporalRegistryEvictorCache;

impl Mappable for DaCompressionTemporalRegistryEvictorCache {
    type Key = Self::OwnedKey;
    type OwnedKey = MetadataKey;
    type Value = Self::OwnedValue;
    type OwnedValue = RegistryKey;
}

impl TableWithBlueprint for DaCompressionTemporalRegistryEvictorCache {
    // TODO: Use Raw codec for value instead of Postcard
    type Blueprint = Plain<Postcard, Postcard>;
    type Column = super::Column;

    fn column() -> Self::Column {
        Self::Column::DaCompressionTemporalRegistryEvictorCache
    }
}

macro_rules! temporal_registry {
    ($type:ty, $code:ty) => {
        paste::paste! {
            pub struct [< DaCompressionTemporalRegistry $type >];

            impl Mappable for [< DaCompressionTemporalRegistry $type >] {
                type Key = Self::OwnedKey;
                type OwnedKey = RegistryKey;
                type Value = Self::OwnedValue;
                type OwnedValue = $type;
            }

            impl TableWithBlueprint for [< DaCompressionTemporalRegistry $type >] {
                // TODO: Use Raw codec for value instead of Postcard
                type Blueprint = Plain<Postcard, $code>;
                type Column = super::Column;

                fn column() -> Self::Column {
                    Self::Column::[< DaCompressionTemporalRegistry $type >]
                }
            }


            #[cfg(test)]
            fuel_core_storage::basic_storage_tests!(
                [< DaCompressionTemporalRegistry $type >],
                RegistryKey::ZERO,
                <[< DaCompressionTemporalRegistry $type >] as Mappable>::Value::default(),
                <[< DaCompressionTemporalRegistry $type >] as Mappable>::Value::default(),
                tests::generate_key
            );
        }
    };
}

temporal_registry!(Address, Raw);
temporal_registry!(AssetId, Raw);
temporal_registry!(ContractId, Raw);
temporal_registry!(ScriptCode, ScriptCodeCodec);
temporal_registry!(PredicateCode, PredicateCodeCodec);

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(test)]
    fuel_core_storage::basic_storage_tests!(
        DaCompressionTemporalRegistryIndex,
        ReverseKey::Address(Address::zeroed()),
        RegistryKey::ZERO
    );

    #[cfg(test)]
    fuel_core_storage::basic_storage_tests!(
        DaCompressionTemporalRegistryEvictorCache,
        MetadataKey::Address,
        RegistryKey::ZERO
    );

    fuel_core_storage::basic_storage_tests!(
        DaCompressedBlocks,
        <DaCompressedBlocks as Mappable>::Key::default(),
        <DaCompressedBlocks as Mappable>::Value::default()
    );

    #[allow(clippy::arithmetic_side_effects)] // Test code, and also safe
    pub fn generate_key(rng: &mut impl rand::Rng) -> RegistryKey {
        let raw_key: u32 = rng.gen_range(0..2u32.pow(24) - 2);
        RegistryKey::try_from(raw_key).unwrap()
    }
}
