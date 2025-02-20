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
        Bytes32,
        ContractId,
    },
};

pub struct TemporalRegistryContractIdMerkleData;

impl Mappable for TemporalRegistryContractIdMerkleData {
    type Key = u64;
    type OwnedKey = Self::Key;
    type Value = binary::Primitive;
    type OwnedValue = Self::Value;
}

impl TableWithBlueprint for TemporalRegistryContractIdMerkleData {
    type Blueprint = Plain<Primitive<8>, Postcard>;
    type Column = Column;

    fn column() -> Column {
        Column::DaCompressionTemporalContractIdMerkleData
    }
}

/// The metadata table for [`TemporalRegistryContractIdMerkleData`] table.
pub struct TemporalRegistryContractIdMerkleMetadata;

impl Mappable for TemporalRegistryContractIdMerkleMetadata {
    type Key = DenseMetadataKey<RegistryKey>;
    type OwnedKey = Self::Key;
    type Value = DenseMerkleMetadata;
    type OwnedValue = Self::Value;
}

impl TableWithBlueprint for TemporalRegistryContractIdMerkleMetadata {
    type Blueprint = Plain<Postcard, Postcard>;
    type Column = Column;

    fn column() -> Column {
        Column::DaCompressionTemporalContractIdMerkleMetadata
    }
}

/// Encoder for the V2 version of the DaCompressionTemporalRegistry for ContractId.
pub struct DaCompressionTemporalRegistryContractIdV2Encoder;

impl fuel_core_storage::codec::Encode<ContractId>
    for DaCompressionTemporalRegistryContractIdV2Encoder
{
    type Encoder<'a> = [u8; Bytes32::LEN];
    fn encode(value: &ContractId) -> Self::Encoder<'_> {
        *Borrow::<[u8; Bytes32::LEN]>::borrow(value)
    }
}

/// V2 table for storing ContractId with Merklized encoding.
pub struct DaCompressionTemporalRegistryContractIdV2;

impl Mappable for DaCompressionTemporalRegistryContractIdV2 {
    type Key = Self::OwnedKey;
    type OwnedKey = RegistryKey;
    type Value = Self::OwnedValue;
    type OwnedValue = ContractId;
}

impl TableWithBlueprint for DaCompressionTemporalRegistryContractIdV2 {
    type Blueprint = Merklized<
        Postcard,
        Raw,
        TemporalRegistryContractIdMerkleMetadata,
        TemporalRegistryContractIdMerkleData,
        DaCompressionTemporalRegistryContractIdV2Encoder,
    >;
    type Column = Column;

    fn column() -> Self::Column {
        Column::DaCompressionTemporalRegistryContractIdV2
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fuel_core_graphql_api::storage::da_compression::tests::generate_key;

    #[cfg(test)]
    fuel_core_storage::basic_merklelized_storage_tests!(
        DaCompressionTemporalRegistryContractIdV2,
        RegistryKey::ZERO,
        <DaCompressionTemporalRegistryContractIdV2 as Mappable>::Value::default(),
        <DaCompressionTemporalRegistryContractIdV2 as Mappable>::Value::default(),
        generate_key
    );
}
