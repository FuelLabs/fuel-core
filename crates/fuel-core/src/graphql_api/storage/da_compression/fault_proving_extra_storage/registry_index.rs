use crate::graphql_api::storage::{
    da_compression::{
        reverse_key::ReverseKey,
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

pub struct TemporalRegistryIndexMerkleData;

impl Mappable for TemporalRegistryIndexMerkleData {
    type Key = u64;
    type OwnedKey = Self::Key;
    type Value = binary::Primitive;
    type OwnedValue = Self::Value;
}

impl TableWithBlueprint for TemporalRegistryIndexMerkleData {
    type Blueprint = Plain<Primitive<8>, Postcard>;
    type Column = Column;

    fn column() -> Column {
        Column::DaCompressionTemporalRegistryIndexMerkleData
    }
}

/// The metadata table for [`TemporalRegistryIndexMerkleData`] table.
pub struct TemporalRegistryIndexMerkleMetadata;

impl Mappable for TemporalRegistryIndexMerkleMetadata {
    type Key = DenseMetadataKey<ReverseKey>;
    type OwnedKey = Self::Key;
    type Value = DenseMerkleMetadata;
    type OwnedValue = Self::Value;
}

impl TableWithBlueprint for TemporalRegistryIndexMerkleMetadata {
    type Blueprint = Plain<Postcard, Postcard>;
    type Column = Column;

    fn column() -> Column {
        Column::DaCompressionTemporalRegistryIndexMerkleMetadata
    }
}

/// Encoder for the V2 version of the DaCompressionTemporalRegistry for RegistryIndex.
pub struct DaCompressionTemporalRegistryIndexV2Encoder;

impl fuel_core_storage::codec::Encode<RegistryKey>
    for DaCompressionTemporalRegistryIndexV2Encoder
{
    type Encoder<'a> = [u8; RegistryKey::SIZE];

    fn encode(value: &RegistryKey) -> Self::Encoder<'_> {
        let mut bytes = [0u8; RegistryKey::SIZE];
        bytes.copy_from_slice(value.as_ref());
        bytes
    }
}

/// Mapping from the type to the registry key in the temporal registry.
pub struct DaCompressionTemporalRegistryIndexV2;

impl Mappable for DaCompressionTemporalRegistryIndexV2 {
    type Key = Self::OwnedKey;
    type OwnedKey = ReverseKey;
    type Value = Self::OwnedValue;
    type OwnedValue = RegistryKey;
}

impl TableWithBlueprint for DaCompressionTemporalRegistryIndexV2 {
    type Blueprint = Merklized<
        Postcard,
        Postcard,
        TemporalRegistryIndexMerkleMetadata,
        TemporalRegistryIndexMerkleData,
        DaCompressionTemporalRegistryIndexV2Encoder,
    >;
    type Column = Column;

    fn column() -> Self::Column {
        Self::Column::DaCompressionTemporalRegistryIndexV2
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fuel_core_types::fuel_tx::Address;

    #[cfg(test)]
    fuel_core_storage::basic_merklelized_storage_tests!(
        DaCompressionTemporalRegistryIndexV2,
        ReverseKey::Address(Address::zeroed()),
        RegistryKey::ZERO
    );
}
