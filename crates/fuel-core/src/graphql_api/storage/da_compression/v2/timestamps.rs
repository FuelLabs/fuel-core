use crate::graphql_api::storage::{
    da_compression::timestamps::TimestampKey,
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
use fuel_core_types::{
    fuel_merkle::binary,
    tai64::Tai64,
};

pub struct TemporalRegistryTimestampsMerkleData;

impl Mappable for TemporalRegistryTimestampsMerkleData {
    type Key = u64;
    type OwnedKey = Self::Key;
    type Value = binary::Primitive;
    type OwnedValue = Self::Value;
}

impl TableWithBlueprint for TemporalRegistryTimestampsMerkleData {
    type Blueprint = Plain<Primitive<8>, Postcard>;
    type Column = Column;

    fn column() -> Self::Column {
        Self::Column::DaCompressionTemporalRegistryTimestampsMerkleData
    }
}

/// The metadata table for [`TemporalRegistryTimestampsMerkleData`] table.
pub struct TemporalRegistryTimestampsMerkleMetadata;

impl Mappable for TemporalRegistryTimestampsMerkleMetadata {
    type Key = DenseMetadataKey<TimestampKey>;
    type OwnedKey = Self::Key;
    type Value = DenseMerkleMetadata;
    type OwnedValue = Self::Value;
}

impl TableWithBlueprint for TemporalRegistryTimestampsMerkleMetadata {
    type Blueprint = Plain<Postcard, Postcard>;
    type Column = Column;

    fn column() -> Self::Column {
        Self::Column::DaCompressionTemporalRegistryTimestampsMerkleMetadata
    }
}

/// Encoder for the V2 version of the DaCompressionTemporalRegistry for Timestamps.
pub struct DaCompressionTemporalRegistryTimestampsV2Encoder;

impl fuel_core_storage::codec::Encode<Tai64>
    for DaCompressionTemporalRegistryTimestampsV2Encoder
{
    type Encoder<'a> = [u8; Tai64::BYTE_SIZE];

    fn encode(value: &Tai64) -> Self::Encoder<'_> {
        value.to_bytes()
    }
}

/// This table keeps track of last written timestamp for each key,
/// so that we can keep track of expiration.
pub struct DaCompressionTemporalRegistryTimestampsV2;

impl Mappable for DaCompressionTemporalRegistryTimestampsV2 {
    type Key = Self::OwnedKey;
    type OwnedKey = TimestampKey;
    type Value = Self::OwnedValue;
    type OwnedValue = Tai64;
}

impl TableWithBlueprint for DaCompressionTemporalRegistryTimestampsV2 {
    type Blueprint = Merklized<
        Postcard,
        Postcard,
        TemporalRegistryTimestampsMerkleMetadata,
        TemporalRegistryTimestampsMerkleData,
        DaCompressionTemporalRegistryTimestampsV2Encoder,
    >;
    type Column = Column;

    fn column() -> Self::Column {
        Self::Column::DaCompressionTemporalRegistryTimestampsV2
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graphql_api::storage::da_compression::{
        timestamps::{
            TimestampKey,
            TimestampKeyspace,
        },
        RegistryKey,
    };

    #[cfg(test)]
    fuel_core_storage::basic_merklelized_storage_tests!(
        DaCompressionTemporalRegistryTimestampsV2,
        TimestampKey {
            keyspace: TimestampKeyspace::Address,
            key: RegistryKey::ZERO
        },
        Tai64::UNIX_EPOCH
    );
}
