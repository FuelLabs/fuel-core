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

#[cfg(feature = "fault-proving")]
pub mod v2 {
    use super::{
        super::Column,
        *,
    };

    use core::borrow::Borrow;
    use fuel_core_storage::{
        blueprint::merklized::Merklized,
        tables::merkle::{
            DenseMerkleMetadata,
            DenseMetadataKey,
        },
    };
    use fuel_core_types::{
        fuel_merkle::binary,
        fuel_tx::Bytes32,
    };

    pub struct TemporalRegistryAddressMerkleData;

    impl Mappable for TemporalRegistryAddressMerkleData {
        type Key = u64;
        type OwnedKey = Self::Key;
        type Value = binary::Primitive;
        type OwnedValue = Self::Value;
    }

    /// The metadata table for [`TemporalRegistryAddressMerkleData`] table.
    pub struct TemporalRegistryAddressMerkleMetadata;

    impl Mappable for TemporalRegistryAddressMerkleMetadata {
        type Key = DenseMetadataKey<RegistryKey>;
        type OwnedKey = Self::Key;
        type Value = DenseMerkleMetadata;
        type OwnedValue = Self::Value;
    }

    /// V2 table for storing Address with Merklized encoding.
    pub struct DaCompressionTemporalRegistryAddressV2;

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
            Self::Column::DaCompressionTemporalRegistryAddressV2
        }
    }

    impl TableWithBlueprint for TemporalRegistryAddressMerkleData {
        type Blueprint = Plain<Primitive<8>, Postcard>;
        type Column = Column;

        fn column() -> Column {
            Column::DaCompressionTemporalAddressMerkleData
        }
    }

    impl TableWithBlueprint for TemporalRegistryAddressMerkleMetadata {
        type Blueprint = Plain<Postcard, Postcard>;
        type Column = Column;

        fn column() -> Column {
            Column::DaCompressionTemporalAddressMerkleMetadata
        }
    }

    /// V2 table for storing AssetId with Merklized encoding.
    pub struct DaCompressionTemporalRegistryAssetIdV2;

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

    pub struct TemporalRegistryAssetIdMerkleData;

    impl Mappable for TemporalRegistryAssetIdMerkleData {
        type Key = u64;
        type OwnedKey = Self::Key;
        type Value = binary::Primitive;
        type OwnedValue = Self::Value;
    }

    /// The metadata table for [`TemporalRegistryAssetIdMerkleData`] table.
    pub struct TemporalRegistryAssetIdMerkleMetadata;

    impl Mappable for TemporalRegistryAssetIdMerkleMetadata {
        type Key = DenseMetadataKey<RegistryKey>;
        type OwnedKey = Self::Key;
        type Value = DenseMerkleMetadata;
        type OwnedValue = Self::Value;
    }

    impl TableWithBlueprint for TemporalRegistryAssetIdMerkleData {
        type Blueprint = Plain<Primitive<8>, Postcard>;
        type Column = Column;

        fn column() -> Column {
            Column::DaCompressionTemporalAssetIdMerkleData
        }
    }

    impl TableWithBlueprint for TemporalRegistryAssetIdMerkleMetadata {
        type Blueprint = Plain<Postcard, Postcard>;
        type Column = Column;

        fn column() -> Column {
            Column::DaCompressionTemporalAssetIdMerkleMetadata
        }
    }

    /// V2 table for storing ContractId with Merklized encoding.
    pub struct DaCompressionTemporalRegistryContractIdV2;

    /// Encoder for the V2 version of the DaCompressionTemporalRegistry for ContractId.
    pub struct DaCompressionTemporalRegistryContractIdV2Encoder;

    pub struct TemporalRegistryContractIdMerkleData;

    impl Mappable for TemporalRegistryContractIdMerkleData {
        type Key = u64;
        type OwnedKey = Self::Key;
        type Value = binary::Primitive;
        type OwnedValue = Self::Value;
    }

    /// The metadata table for [`TemporalRegistryContractIdMerkleData`] table.
    pub struct TemporalRegistryContractIdMerkleMetadata;

    impl Mappable for TemporalRegistryContractIdMerkleMetadata {
        type Key = DenseMetadataKey<RegistryKey>;
        type OwnedKey = Self::Key;
        type Value = DenseMerkleMetadata;
        type OwnedValue = Self::Value;
    }

    impl TableWithBlueprint for TemporalRegistryContractIdMerkleData {
        type Blueprint = Plain<Primitive<8>, Postcard>;
        type Column = Column;

        fn column() -> Column {
            Column::DaCompressionTemporalContractIdMerkleData
        }
    }

    impl TableWithBlueprint for TemporalRegistryContractIdMerkleMetadata {
        type Blueprint = Plain<Postcard, Postcard>;
        type Column = Column;

        fn column() -> Column {
            Column::DaCompressionTemporalContractIdMerkleMetadata
        }
    }

    impl fuel_core_storage::codec::Encode<ContractId>
        for DaCompressionTemporalRegistryContractIdV2Encoder
    {
        type Encoder<'a> = [u8; Bytes32::LEN];
        fn encode(value: &ContractId) -> Self::Encoder<'_> {
            *Borrow::<[u8; Bytes32::LEN]>::borrow(value)
        }
    }

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
            Self::Column::DaCompressionTemporalRegistryContractIdV2
        }
    }

    /// V2 table for storing ScriptCode with Merklized encoding.
    pub struct DaCompressionTemporalRegistryScriptCodeV2;

    /// Encoder for the V2 version of the DaCompressionTemporalRegistry for ScriptCode.
    pub struct DaCompressionTemporalRegistryScriptCodeV2Encoder;

    pub struct TemporalRegistryScriptCodeMerkleData;

    impl Mappable for TemporalRegistryScriptCodeMerkleData {
        type Key = u64;
        type OwnedKey = Self::Key;
        type Value = binary::Primitive;
        type OwnedValue = Self::Value;
    }

    /// The metadata table for [`TemporalRegistryScriptCodeMerkleData`] table.
    pub struct TemporalRegistryScriptCodeMerkleMetadata;

    impl Mappable for TemporalRegistryScriptCodeMerkleMetadata {
        type Key = DenseMetadataKey<RegistryKey>;
        type OwnedKey = Self::Key;
        type Value = DenseMerkleMetadata;
        type OwnedValue = Self::Value;
    }

    impl TableWithBlueprint for TemporalRegistryScriptCodeMerkleData {
        type Blueprint = Plain<Primitive<8>, Postcard>;
        type Column = Column;

        fn column() -> Column {
            Column::DaCompressionTemporalScriptCodeMerkleData
        }
    }

    impl TableWithBlueprint for TemporalRegistryScriptCodeMerkleMetadata {
        type Blueprint = Plain<Postcard, Postcard>;
        type Column = Column;

        fn column() -> Column {
            Column::DaCompressionTemporalScriptCodeMerkleMetadata
        }
    }

    impl fuel_core_storage::codec::Encode<ScriptCode>
        for DaCompressionTemporalRegistryScriptCodeV2Encoder
    {
        type Encoder<'a> = std::borrow::Cow<'a, [u8]>;
        fn encode(value: &ScriptCode) -> Self::Encoder<'_> {
            let bytes: Vec<u8> = value.bytes.clone();
            std::borrow::Cow::Owned(bytes)
        }
    }

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
            Self::Column::DaCompressionTemporalRegistryScriptCodeV2
        }
    }

    /// V2 table for storing PredicateCode with Merklized encoding.
    pub struct DaCompressionTemporalRegistryPredicateCodeV2;

    /// Encoder for the V2 version of the DaCompressionTemporalRegistry for PredicateCode.
    pub struct DaCompressionTemporalRegistryPredicateCodeV2Encoder;

    pub struct TemporalRegistryPredicateCodeMerkleData;

    impl Mappable for TemporalRegistryPredicateCodeMerkleData {
        type Key = u64;
        type OwnedKey = Self::Key;
        type Value = binary::Primitive;
        type OwnedValue = Self::Value;
    }

    /// The metadata table for [`TemporalRegistryPredicateCodeMerkleData`] table.
    pub struct TemporalRegistryPredicateCodeMerkleMetadata;

    impl Mappable for TemporalRegistryPredicateCodeMerkleMetadata {
        type Key = DenseMetadataKey<RegistryKey>;
        type OwnedKey = Self::Key;
        type Value = DenseMerkleMetadata;
        type OwnedValue = Self::Value;
    }

    impl TableWithBlueprint for TemporalRegistryPredicateCodeMerkleData {
        type Blueprint = Plain<Primitive<8>, Postcard>;
        type Column = Column;

        fn column() -> Column {
            Column::DaCompressionTemporalPredicateCodeMerkleData
        }
    }

    impl TableWithBlueprint for TemporalRegistryPredicateCodeMerkleMetadata {
        type Blueprint = Plain<Postcard, Postcard>;
        type Column = Column;

        fn column() -> Column {
            Column::DaCompressionTemporalPredicateCodeMerkleMetadata
        }
    }

    impl fuel_core_storage::codec::Encode<PredicateCode>
        for DaCompressionTemporalRegistryPredicateCodeV2Encoder
    {
        type Encoder<'a> = std::borrow::Cow<'a, [u8]>;
        fn encode(value: &PredicateCode) -> Self::Encoder<'_> {
            let bytes: Vec<u8> = value.bytes.clone();
            std::borrow::Cow::Owned(bytes)
        }
    }

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
            Self::Column::DaCompressionTemporalRegistryPredicateCodeV2
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

        #[cfg(test)]
        fuel_core_storage::basic_merklelized_storage_tests!(
            DaCompressionTemporalRegistryAssetIdV2,
            RegistryKey::ZERO,
            <DaCompressionTemporalRegistryAssetIdV2 as Mappable>::Value::default(),
            <DaCompressionTemporalRegistryAssetIdV2 as Mappable>::Value::default(),
            generate_key
        );

        #[cfg(test)]
        fuel_core_storage::basic_merklelized_storage_tests!(
            DaCompressionTemporalRegistryContractIdV2,
            RegistryKey::ZERO,
            <DaCompressionTemporalRegistryContractIdV2 as Mappable>::Value::default(),
            <DaCompressionTemporalRegistryContractIdV2 as Mappable>::Value::default(),
            generate_key
        );

        #[cfg(test)]
        fuel_core_storage::basic_merklelized_storage_tests!(
            DaCompressionTemporalRegistryScriptCodeV2,
            RegistryKey::ZERO,
            <DaCompressionTemporalRegistryScriptCodeV2 as Mappable>::Value::default(),
            <DaCompressionTemporalRegistryScriptCodeV2 as Mappable>::Value::default(),
            generate_key
        );

        #[cfg(test)]
        fuel_core_storage::basic_merklelized_storage_tests!(
            DaCompressionTemporalRegistryPredicateCodeV2,
            RegistryKey::ZERO,
            <DaCompressionTemporalRegistryPredicateCodeV2 as Mappable>::Value::default(),
            <DaCompressionTemporalRegistryPredicateCodeV2 as Mappable>::Value::default(),
            generate_key
        );
    }
}

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
        DaCompressionTemporalRegistryTimestamps,
        TimestampKey {
            keyspace: timestamps::TimestampKeyspace::Address,
            key: RegistryKey::ZERO
        },
        Tai64::UNIX_EPOCH
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
