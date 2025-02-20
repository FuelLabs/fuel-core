use crate::graphql_api::storage::{
    da_compression::{
        PredicateCode,
        PredicateCodeCodec,
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

pub struct TemporalRegistryPredicateCodeMerkleData;

impl Mappable for TemporalRegistryPredicateCodeMerkleData {
    type Key = u64;
    type OwnedKey = Self::Key;
    type Value = binary::Primitive;
    type OwnedValue = Self::Value;
}

impl TableWithBlueprint for TemporalRegistryPredicateCodeMerkleData {
    type Blueprint = Plain<Primitive<8>, Postcard>;
    type Column = Column;

    fn column() -> Column {
        Column::DaCompressionTemporalPredicateCodeMerkleData
    }
}

/// The metadata table for [`TemporalRegistryPredicateCodeMerkleData`] table.
pub struct TemporalRegistryPredicateCodeMerkleMetadata;

impl Mappable for TemporalRegistryPredicateCodeMerkleMetadata {
    type Key = DenseMetadataKey<RegistryKey>;
    type OwnedKey = Self::Key;
    type Value = DenseMerkleMetadata;
    type OwnedValue = Self::Value;
}

impl TableWithBlueprint for TemporalRegistryPredicateCodeMerkleMetadata {
    type Blueprint = Plain<Postcard, Postcard>;
    type Column = Column;

    fn column() -> Column {
        Column::DaCompressionTemporalPredicateCodeMerkleMetadata
    }
}

/// Encoder for the V2 version of the DaCompressionTemporalRegistry for PredicateCode.
pub struct DaCompressionTemporalRegistryPredicateCodeV2Encoder;

impl fuel_core_storage::codec::Encode<PredicateCode>
    for DaCompressionTemporalRegistryPredicateCodeV2Encoder
{
    type Encoder<'a> = std::borrow::Cow<'a, [u8]>;

    fn encode(value: &PredicateCode) -> Self::Encoder<'_> {
        let bytes: Vec<u8> = value.bytes.clone();
        std::borrow::Cow::Owned(bytes)
    }
}

/// V2 table for storing PredicateCode with Merklized encoding.
pub struct DaCompressionTemporalRegistryPredicateCodeV2;

impl Mappable for DaCompressionTemporalRegistryPredicateCodeV2 {
    type Key = Self::OwnedKey;
    type OwnedKey = RegistryKey;
    type Value = Self::OwnedValue;
    type OwnedValue = PredicateCode;
}

impl TableWithBlueprint for DaCompressionTemporalRegistryPredicateCodeV2 {
    type Blueprint = Merklized<
        Postcard,
        PredicateCodeCodec,
        TemporalRegistryPredicateCodeMerkleMetadata,
        TemporalRegistryPredicateCodeMerkleData,
        DaCompressionTemporalRegistryPredicateCodeV2Encoder,
    >;
    type Column = Column;

    fn column() -> Self::Column {
        Column::DaCompressionTemporalRegistryPredicateCodeV2
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fuel_core_graphql_api::storage::da_compression::tests::generate_key;

    #[cfg(test)]
    fuel_core_storage::basic_merklelized_storage_tests!(
        DaCompressionTemporalRegistryPredicateCodeV2,
        RegistryKey::ZERO,
        <DaCompressionTemporalRegistryPredicateCodeV2 as Mappable>::Value::default(),
        <DaCompressionTemporalRegistryPredicateCodeV2 as Mappable>::Value::default(),
        generate_key
    );
}
