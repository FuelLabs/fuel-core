use fuel_core_storage::{
    blueprint::plain::Plain,
    codec::{
        manual::Manual,
        postcard::Postcard,
        Decode,
        Encode,
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
use std::borrow::Cow;

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

impl Encode<OwnedMessageKey> for Manual<OwnedMessageKey> {
    type Encoder<'a> = Cow<'a, [u8]>;

    fn encode(t: &OwnedMessageKey) -> Self::Encoder<'_> {
        Cow::Borrowed(t.as_ref())
    }
}

impl Decode<OwnedMessageKey> for Manual<OwnedMessageKey> {
    fn decode(bytes: &[u8]) -> anyhow::Result<OwnedMessageKey> {
        OwnedMessageKey::from_slice(bytes)
            .map_err(|_| anyhow::anyhow!("Unable to decode bytes"))
    }
}

impl TableWithBlueprint for OwnedMessageIds {
    type Blueprint = Plain<Manual<OwnedMessageKey>, Postcard>;
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
