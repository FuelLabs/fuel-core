use crate::graphql_api::storage::{
    da_compression::RegistryKey,
    Column,
};
use core::borrow::Borrow;
use fuel_core_storage::{
    blueprint::{
        merklized::Merklized,
        plain::Plain,
    },
    codec::{
        postcard::Postcard,
        primitive::Primitive,
        raw::Raw,
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
    fuel_tx::{
        AssetId,
        Bytes32,
    },
};

pub struct TemporalRegistryAssetIdMerkleData;

impl Mappable for TemporalRegistryAssetIdMerkleData {
    type Key = u64;
    type OwnedKey = Self::Key;
    type Value = binary::Primitive;
    type OwnedValue = Self::Value;
}

impl TableWithBlueprint for TemporalRegistryAssetIdMerkleData {
    type Blueprint = Plain<Primitive<8>, Postcard>;
    type Column = Column;

    fn column() -> Column {
        Column::DaCompressionTemporalAssetIdMerkleData
    }
}

/// The metadata table for [`TemporalRegistryAssetIdMerkleData`] table.
pub struct TemporalRegistryAssetIdMerkleMetadata;

impl Mappable for TemporalRegistryAssetIdMerkleMetadata {
    type Key = DenseMetadataKey<RegistryKey>;
    type OwnedKey = Self::Key;
    type Value = DenseMerkleMetadata;
    type OwnedValue = Self::Value;
}

impl TableWithBlueprint for TemporalRegistryAssetIdMerkleMetadata {
    type Blueprint = Plain<Postcard, Postcard>;
    type Column = Column;

    fn column() -> Column {
        Column::DaCompressionTemporalAssetIdMerkleMetadata
    }
}

/// Encoder for the V2 version of the DaCompressionTemporalRegistry for AssetId.
pub struct DaCompressionTemporalRegistryAssetIdV2Encoder;

impl fuel_core_storage::codec::Encode<AssetId>
    for DaCompressionTemporalRegistryAssetIdV2Encoder
{
    type Encoder<'a> = [u8; Bytes32::LEN];
    fn encode(value: &AssetId) -> Self::Encoder<'_> {
        *Borrow::<[u8; Bytes32::LEN]>::borrow(value)
    }
}

/// V2 table for storing AssetId with Merklized encoding.
pub struct DaCompressionTemporalRegistryAssetIdV2;

impl Mappable for DaCompressionTemporalRegistryAssetIdV2 {
    type Key = Self::OwnedKey;
    type OwnedKey = RegistryKey;
    type Value = Self::OwnedValue;
    type OwnedValue = AssetId;
}

impl TableWithBlueprint for DaCompressionTemporalRegistryAssetIdV2 {
    type Blueprint = Merklized<
        Postcard,
        Raw,
        TemporalRegistryAssetIdMerkleMetadata,
        TemporalRegistryAssetIdMerkleData,
        DaCompressionTemporalRegistryAssetIdV2Encoder,
    >;
    type Column = Column;
    fn column() -> Self::Column {
        Self::Column::DaCompressionTemporalRegistryAssetIdV2
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fuel_core_graphql_api::storage::da_compression::tests::generate_key;

    #[cfg(test)]
    fuel_core_storage::basic_merklelized_storage_tests!(
        DaCompressionTemporalRegistryAssetIdV2,
        RegistryKey::ZERO,
        <DaCompressionTemporalRegistryAssetIdV2 as Mappable>::Value::default(),
        <DaCompressionTemporalRegistryAssetIdV2 as Mappable>::Value::default(),
        generate_key
    );
}
