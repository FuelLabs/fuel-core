use crate::graphql_api::storage::{
    da_compression::{
        RegistryKey,
        ScriptCodeCodec,
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
use fuel_core_types::{
    fuel_merkle::binary,
    fuel_tx::ScriptCode,
};

pub struct TemporalRegistryScriptCodeMerkleData;

impl Mappable for TemporalRegistryScriptCodeMerkleData {
    type Key = u64;
    type OwnedKey = Self::Key;
    type Value = binary::Primitive;
    type OwnedValue = Self::Value;
}

impl TableWithBlueprint for TemporalRegistryScriptCodeMerkleData {
    type Blueprint = Plain<Primitive<8>, Postcard>;
    type Column = Column;

    fn column() -> Column {
        Column::DaCompressionTemporalScriptCodeMerkleData
    }
}

/// The metadata table for [`TemporalRegistryScriptCodeMerkleData`] table.
pub struct TemporalRegistryScriptCodeMerkleMetadata;

impl Mappable for TemporalRegistryScriptCodeMerkleMetadata {
    type Key = DenseMetadataKey<RegistryKey>;
    type OwnedKey = Self::Key;
    type Value = DenseMerkleMetadata;
    type OwnedValue = Self::Value;
}

impl TableWithBlueprint for TemporalRegistryScriptCodeMerkleMetadata {
    type Blueprint = Plain<Postcard, Postcard>;
    type Column = Column;

    fn column() -> Column {
        Column::DaCompressionTemporalScriptCodeMerkleMetadata
    }
}

/// Encoder for the V2 version of the DaCompressionTemporalRegistry for ScriptCode.
pub struct DaCompressionTemporalRegistryScriptCodeV2Encoder;

impl fuel_core_storage::codec::Encode<ScriptCode>
    for DaCompressionTemporalRegistryScriptCodeV2Encoder
{
    type Encoder<'a> = std::borrow::Cow<'a, [u8]>;

    fn encode(value: &ScriptCode) -> Self::Encoder<'_> {
        let bytes: Vec<u8> = value.bytes.clone();
        std::borrow::Cow::Owned(bytes)
    }
}

/// V2 table for storing ScriptCode with Merklized encoding.
pub struct DaCompressionTemporalRegistryScriptCodeV2;

impl Mappable for DaCompressionTemporalRegistryScriptCodeV2 {
    type Key = Self::OwnedKey;
    type OwnedKey = RegistryKey;
    type Value = Self::OwnedValue;
    type OwnedValue = ScriptCode;
}

impl TableWithBlueprint for DaCompressionTemporalRegistryScriptCodeV2 {
    type Blueprint = Merklized<
        Postcard,
        ScriptCodeCodec,
        TemporalRegistryScriptCodeMerkleMetadata,
        TemporalRegistryScriptCodeMerkleData,
        DaCompressionTemporalRegistryScriptCodeV2Encoder,
    >;
    type Column = Column;

    fn column() -> Self::Column {
        Column::DaCompressionTemporalRegistryScriptCodeV2
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fuel_core_graphql_api::storage::da_compression::tests::generate_key;

    #[cfg(test)]
    fuel_core_storage::basic_merklelized_storage_tests!(
        DaCompressionTemporalRegistryScriptCodeV2,
        RegistryKey::ZERO,
        <DaCompressionTemporalRegistryScriptCodeV2 as Mappable>::Value::default(),
        <DaCompressionTemporalRegistryScriptCodeV2 as Mappable>::Value::default(),
        generate_key
    );
}
