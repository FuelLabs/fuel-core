use crate::graphql_api::storage::{
    da_compression::{
        MetadataKey,
        RegistryKey,
    },
    Column,
};
use fuel_core_storage::{
    blueprint::{
        merklized::Merklized,
        plain::Plain,
    },
    codec::{
        postcard::Postcard,
        primitive::Primitive,
    },
    structured_storage::TableWithBlueprint,
    tables::merkle::{
        DenseMerkleMetadata,
        DenseMetadataKey,
    },
    Mappable,
};
use fuel_core_types::fuel_merkle::binary;

pub struct TemporalRegistryEvictorCacheMerkleData;

impl Mappable for TemporalRegistryEvictorCacheMerkleData {
    type Key = u64;
    type OwnedKey = Self::Key;
    type Value = binary::Primitive;
    type OwnedValue = Self::Value;
}

impl TableWithBlueprint for TemporalRegistryEvictorCacheMerkleData {
    type Blueprint = Plain<Primitive<8>, Postcard>;
    type Column = Column;

    fn column() -> Self::Column {
        Self::Column::DaCompressionTemporalRegistryEvictorCacheMerkleData
    }
}

/// The metadata table for [`TemporalRegistryEvictorCacheMerkleData`] table.
pub struct TemporalRegistryEvictorCacheMerkleMetadata;

impl Mappable for TemporalRegistryEvictorCacheMerkleMetadata {
    type Key = DenseMetadataKey<MetadataKey>;
    type OwnedKey = Self::Key;
    type Value = DenseMerkleMetadata;
    type OwnedValue = Self::Value;
}

impl TableWithBlueprint for TemporalRegistryEvictorCacheMerkleMetadata {
    type Blueprint = Plain<Postcard, Postcard>;
    type Column = Column;

    fn column() -> Self::Column {
        Self::Column::DaCompressionTemporalRegistryEvictorCacheMerkleMetadata
    }
}

impl Mappable for DaCompressionTemporalRegistryEvictorCacheV2 {
    type Key = Self::OwnedKey;
    type OwnedKey = MetadataKey;
    type Value = Self::OwnedValue;
    type OwnedValue = RegistryKey;
}

/// Encoder for the V2 version of the DaCompressionTemporalRegistry for EvictorCache.
pub struct DaCompressionTemporalEvictorCacheV2Encoder;

impl fuel_core_storage::codec::Encode<RegistryKey>
    for DaCompressionTemporalEvictorCacheV2Encoder
{
    type Encoder<'a> = [u8; RegistryKey::SIZE];

    fn encode(value: &RegistryKey) -> Self::Encoder<'_> {
        let mut bytes = [0u8; RegistryKey::SIZE];
        bytes.copy_from_slice(value.as_ref());
        bytes
    }
}

/// This table is used to hold "next key to evict" for each keyspace.
/// In the future we'll likely switch to use LRU or something, in which
/// case this table can be repurposed.
pub struct DaCompressionTemporalRegistryEvictorCacheV2;

impl TableWithBlueprint for DaCompressionTemporalRegistryEvictorCacheV2 {
    type Blueprint = Merklized<
        Postcard,
        Postcard,
        TemporalRegistryEvictorCacheMerkleMetadata,
        TemporalRegistryEvictorCacheMerkleData,
        DaCompressionTemporalEvictorCacheV2Encoder,
    >;
    type Column = Column;

    fn column() -> Self::Column {
        Self::Column::DaCompressionTemporalRegistryEvictorCacheV2
    }
}
