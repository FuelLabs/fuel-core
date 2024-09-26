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
};

pub struct DaCompressedBlocks;

impl Mappable for DaCompressedBlocks {
    type Key = Self::OwnedKey;
    type OwnedKey = BlockHeight;
    type Value = Self::OwnedValue;
    type OwnedValue = Vec<u8>;
}

impl TableWithBlueprint for DaCompressedBlocks {
    type Blueprint = Plain<Primitive<4>, Raw>;
    type Column = super::Column;

    fn column() -> Self::Column {
        Self::Column::DaCompressedBlocks
    }
}

macro_rules! temporal_registry {
    ($type:ty) => {
        paste::paste! {
            pub struct [< DaCompressionTemporalRegistry $type >];

            impl Mappable for [< DaCompressionTemporalRegistry $type >] {
                type Key = Self::OwnedKey;
                type OwnedKey = RegistryKey;
                type Value = Self::OwnedValue;
                type OwnedValue = $type;
            }

            impl TableWithBlueprint for [< DaCompressionTemporalRegistry $type >] {
                type Blueprint = Plain<Postcard, Postcard>;
                type Column = super::Column;

                fn column() -> Self::Column {
                    Self::Column::[< DaCompressionTemporalRegistry $type >]
                }
            }

            pub struct [< DaCompressionTemporalRegistryIndex $type >];

            impl Mappable for [< DaCompressionTemporalRegistryIndex $type >] {
                type Key = Self::OwnedKey;
                type OwnedKey = [u8; 32]; // if the value is larger than 32 bytes, it's hashed
                type Value = Self::OwnedValue;
                type OwnedValue = RegistryKey;
            }

            impl TableWithBlueprint for [< DaCompressionTemporalRegistryIndex $type >] {
                type Blueprint = Plain<Raw, Postcard>;
                type Column = super::Column;

                fn column() -> Self::Column {
                    Self::Column::[< DaCompressionTemporalRegistryIndex $type >]
                }
            }

            /// This table is used to hold "next key to evict" for each keyspace.
            /// In the future we'll likely switch to use LRU or something, in which
            /// case this table can be repurposed.
            pub struct [< DaCompressionTemporalRegistryEvictor $type >];

            impl Mappable for [< DaCompressionTemporalRegistryEvictor $type >] {
                type Key = Self::OwnedKey;
                type OwnedKey = ();
                type Value = Self::OwnedValue;
                type OwnedValue = RegistryKey;
            }

            impl TableWithBlueprint for [< DaCompressionTemporalRegistryEvictor $type >] {
                type Blueprint = Plain<Postcard, Postcard>;
                type Column = super::Column;

                fn column() -> Self::Column {
                    Self::Column::[< DaCompressionTemporalRegistryEvictor $type >]
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

            #[cfg(test)]
            fuel_core_storage::basic_storage_tests!(
                [< DaCompressionTemporalRegistryIndex $type >],
                [0u8; 32],
                RegistryKey::ZERO
            );

            #[cfg(test)]
            fuel_core_storage::basic_storage_tests!(
                [< DaCompressionTemporalRegistryEvictor $type >],
                (),
                RegistryKey::ZERO
            );
        }
    };
}

temporal_registry!(Address);
temporal_registry!(AssetId);
temporal_registry!(ContractId);
temporal_registry!(ScriptCode);
temporal_registry!(PredicateCode);

#[cfg(test)]
mod tests {
    use super::*;

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
