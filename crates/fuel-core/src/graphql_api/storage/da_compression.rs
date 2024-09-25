use fuel_core_compression::{
    RegistryKeyspace,
    RegistryKeyspaceValue,
};
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

pub struct DaCompressionTemporalRegistry;

impl Mappable for DaCompressionTemporalRegistry {
    type Key = Self::OwnedKey;
    type OwnedKey = (RegistryKeyspace, RegistryKey);
    type Value = Self::OwnedValue;
    type OwnedValue = RegistryKeyspaceValue;
}

impl TableWithBlueprint for DaCompressionTemporalRegistry {
    type Blueprint = Plain<Postcard, Postcard>;
    type Column = super::Column;

    fn column() -> Self::Column {
        Self::Column::DaCompressionTemporalRegistry
    }
}

pub struct DaCompressionTemporalRegistryIndex;

impl Mappable for DaCompressionTemporalRegistryIndex {
    type Key = Self::OwnedKey;
    // TODO: should we hash the key?
    type OwnedKey = RegistryKeyspaceValue;
    type Value = Self::OwnedValue;
    type OwnedValue = RegistryKey;
}

impl TableWithBlueprint for DaCompressionTemporalRegistryIndex {
    type Blueprint = Plain<Postcard, Postcard>;
    type Column = super::Column;

    fn column() -> Self::Column {
        Self::Column::DaCompressionTemporalRegistryIndex
    }
}

/// This table is used to hold "next key to evict" for each keyspace.
/// In the future we'll likely switch to use LRU or something, in which
/// case this table can be repurposed, iff migrations have been figured out.
pub struct DaCompressionTemporalRegistryEvictor;

impl Mappable for DaCompressionTemporalRegistryEvictor {
    type Key = Self::OwnedKey;
    type OwnedKey = RegistryKeyspace;
    type Value = Self::OwnedValue;
    type OwnedValue = RegistryKey;
}

impl TableWithBlueprint for DaCompressionTemporalRegistryEvictor {
    type Blueprint = Plain<Postcard, Postcard>;
    type Column = super::Column;

    fn column() -> Self::Column {
        Self::Column::DaCompressionTemporalRegistryEvictor
    }
}

#[cfg(test)]
mod tests {
    use fuel_core_types::fuel_tx::{
        input::PredicateCode,
        ScriptCode,
    };

    use super::*;

    fn generate_keyspace(rng: &mut impl rand::Rng) -> RegistryKeyspace {
        rng.gen()
    }

    #[allow(clippy::arithmetic_side_effects)] // Test code, and also safe
    fn generate_raw_key(rng: &mut impl rand::Rng) -> RegistryKey {
        let raw_key: u32 = rng.gen_range(0..2u32.pow(24) - 2);
        RegistryKey::try_from(raw_key).unwrap()
    }

    fn generate_registry_key(
        rng: &mut impl rand::Rng,
    ) -> (RegistryKeyspace, RegistryKey) {
        (generate_keyspace(rng), generate_raw_key(rng))
    }

    fn generate_registry_index_key(rng: &mut impl rand::Rng) -> RegistryKeyspaceValue {
        let mut bytes: Vec<u8> = vec![0u8; rng.gen_range(0..1234)];
        rng.fill(bytes.as_mut_slice());

        match rng.gen() {
            RegistryKeyspace::address => RegistryKeyspaceValue::address(rng.gen()),
            RegistryKeyspace::asset_id => RegistryKeyspaceValue::asset_id(rng.gen()),
            RegistryKeyspace::contract_id => {
                RegistryKeyspaceValue::contract_id(rng.gen())
            }
            RegistryKeyspace::script_code => {
                RegistryKeyspaceValue::script_code(ScriptCode { bytes })
            }
            RegistryKeyspace::predicate_code => {
                RegistryKeyspaceValue::predicate_code(PredicateCode { bytes })
            }
        }
    }

    fuel_core_storage::basic_storage_tests!(
        DaCompressedBlocks,
        <DaCompressedBlocks as Mappable>::Key::default(),
        <DaCompressedBlocks as Mappable>::Value::default()
    );

    fuel_core_storage::basic_storage_tests!(
        DaCompressionTemporalRegistry,
        (RegistryKeyspace::address, RegistryKey::ZERO),
        RegistryKeyspaceValue::address(fuel_core_types::fuel_tx::Address::zeroed()),
        RegistryKeyspaceValue::address(fuel_core_types::fuel_tx::Address::zeroed()),
        generate_registry_key
    );

    fuel_core_storage::basic_storage_tests!(
        DaCompressionTemporalRegistryIndex,
        RegistryKeyspaceValue::address(fuel_core_types::fuel_tx::Address::zeroed()),
        RegistryKey::ZERO,
        RegistryKey::ZERO,
        generate_registry_index_key
    );

    fuel_core_storage::basic_storage_tests!(
        DaCompressionTemporalRegistryEvictor,
        RegistryKeyspace::address,
        RegistryKey::ZERO,
        RegistryKey::ZERO,
        generate_keyspace
    );
}
