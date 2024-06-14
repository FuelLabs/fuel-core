use fuel_core_chain_config::{
    AddTable,
    AsTable,
    StateConfig,
    StateConfigBuilder,
    TableEntry,
};
use fuel_core_storage::{
    blueprint::plain::Plain,
    codec::{
        postcard::Postcard,
        raw::Raw,
    },
    structured_storage::TableWithBlueprint,
    Mappable,
};
use fuel_core_types::fuel_types::{
    Address,
    Nonce,
};
use rand::{
    distributions::{
        Distribution,
        Standard,
    },
    Rng,
};

fuel_core_types::fuel_vm::double_key!(OwnedMessageKey, Address, address, Nonce, nonce);

impl Distribution<OwnedMessageKey> for Standard {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> OwnedMessageKey {
        let mut bytes = [0u8; 64];

        rng.fill_bytes(bytes.as_mut());

        OwnedMessageKey::from_array(bytes)
    }
}

/// The table that stores all messages per owner.
pub struct OwnedMessageIds;

impl Mappable for OwnedMessageIds {
    type Key = OwnedMessageKey;
    type OwnedKey = Self::Key;
    type Value = ();
    type OwnedValue = Self::Value;
}

impl TableWithBlueprint for OwnedMessageIds {
    type Blueprint = Plain<Raw, Postcard>;
    type Column = super::Column;

    fn column() -> Self::Column {
        Self::Column::OwnedMessageIds
    }
}

#[cfg(test)]
fuel_core_storage::basic_storage_tests!(
    OwnedMessageIds,
    <OwnedMessageIds as Mappable>::Key::default(),
    <OwnedMessageIds as Mappable>::Value::default()
);

/// The storage table that indicates if the message is spent or not.
pub struct SpentMessages;

impl Mappable for SpentMessages {
    type Key = Self::OwnedKey;
    type OwnedKey = Nonce;
    type Value = Self::OwnedValue;
    type OwnedValue = ();
}

impl TableWithBlueprint for SpentMessages {
    type Blueprint = Plain<Raw, Postcard>;
    type Column = super::Column;

    fn column() -> Self::Column {
        Self::Column::SpentMessages
    }
}

impl AsTable<SpentMessages> for StateConfig {
    fn as_table(&self) -> Vec<TableEntry<SpentMessages>> {
        Vec::new() // Do not include these for now
    }
}

impl AddTable<SpentMessages> for StateConfigBuilder {
    fn add(&mut self, _entries: Vec<TableEntry<SpentMessages>>) {
        // Do not include these for now
    }
}

#[cfg(test)]
fuel_core_storage::basic_storage_tests!(
    SpentMessages,
    <SpentMessages as Mappable>::Key::default(),
    <SpentMessages as Mappable>::Value::default()
);
