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
        Address,
        Bytes32,
    },
};

pub struct TemporalRegistryAddressMerkleData;

impl Mappable for TemporalRegistryAddressMerkleData {
    type Key = u64;
    type OwnedKey = Self::Key;
    type Value = binary::Primitive;
    type OwnedValue = Self::Value;
}

impl TableWithBlueprint for TemporalRegistryAddressMerkleData {
    type Blueprint = Plain<Primitive<8>, Postcard>;
    type Column = Column;

    fn column() -> Column {
        Column::DaCompressionTemporalAddressMerkleData
    }
}

/// The metadata table for [`TemporalRegistryAddressMerkleData`] table.
pub struct TemporalRegistryAddressMerkleMetadata;

impl Mappable for TemporalRegistryAddressMerkleMetadata {
    type Key = DenseMetadataKey<RegistryKey>;
    type OwnedKey = Self::Key;
    type Value = DenseMerkleMetadata;
    type OwnedValue = Self::Value;
}

impl TableWithBlueprint for TemporalRegistryAddressMerkleMetadata {
    type Blueprint = Plain<Postcard, Postcard>;
    type Column = Column;

    fn column() -> Column {
        Column::DaCompressionTemporalAddressMerkleMetadata
    }
}

/// Encoder for the V2 version of the DaCompressionTemporalRegistry for Address.
pub struct DaCompressionTemporalRegistryAddressV2Encoder;

impl fuel_core_storage::codec::Encode<Address>
    for DaCompressionTemporalRegistryAddressV2Encoder
{
    type Encoder<'a> = [u8; Bytes32::LEN];

    fn encode(value: &Address) -> Self::Encoder<'_> {
        *Borrow::<[u8; Bytes32::LEN]>::borrow(value)
    }
}

/// V2 table for storing Address with Merklized encoding.
pub struct DaCompressionTemporalRegistryAddressV2;

impl Mappable for DaCompressionTemporalRegistryAddressV2 {
    type Key = Self::OwnedKey;
    type OwnedKey = RegistryKey;
    type Value = Self::OwnedValue;
    type OwnedValue = Address;
}

impl TableWithBlueprint for DaCompressionTemporalRegistryAddressV2 {
    type Blueprint = Merklized<
        Postcard,
        Raw,
        TemporalRegistryAddressMerkleMetadata,
        TemporalRegistryAddressMerkleData,
        DaCompressionTemporalRegistryAddressV2Encoder,
    >;
    type Column = Column;

    fn column() -> Self::Column {
        Column::DaCompressionTemporalRegistryAddressV2
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fuel_core_graphql_api::storage::da_compression::tests::generate_key;

    #[cfg(test)]
    fuel_core_storage::basic_merklelized_storage_tests!(
        DaCompressionTemporalRegistryAddressV2,
        RegistryKey::ZERO,
        <DaCompressionTemporalRegistryAddressV2 as Mappable>::Value::default(),
        <DaCompressionTemporalRegistryAddressV2 as Mappable>::Value::default(),
        generate_key
    );
}
