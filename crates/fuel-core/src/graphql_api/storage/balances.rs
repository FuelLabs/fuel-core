use fuel_core_storage::{
    blueprint::plain::Plain,
    codec::{
        postcard::Postcard,
        raw::Raw,
    },
    structured_storage::TableWithBlueprint,
    Mappable,
};
use fuel_core_types::{
    fuel_tx::{
        Address,
        AssetId,
    },
    fuel_vm::double_key,
};
use rand::{
    distributions::Standard,
    prelude::Distribution,
    Rng,
};

pub type Amount = u64;

double_key!(BalancesKey, Address, address, AssetId, asset_id);
impl Distribution<BalancesKey> for Standard {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> BalancesKey {
        let mut bytes = [0u8; BalancesKey::LEN];
        rng.fill_bytes(bytes.as_mut());
        BalancesKey::from_array(bytes)
    }
}

/// These table stores the balances of coins per owner and asset id.
pub struct Balances;

impl Mappable for Balances {
    type Key = BalancesKey;
    type OwnedKey = Self::Key;
    type Value = Amount;
    type OwnedValue = Self::Value;
}

impl TableWithBlueprint for Balances {
    type Blueprint = Plain<Raw, Postcard>; // TODO[RC]: What is Plain, Raw, Postcard, Primitive<N> and others in this context?
    type Column = super::Column;

    fn column() -> Self::Column {
        Self::Column::CoinBalances
    }
}

/// These table stores the balances of messages per owner.
pub struct MessageBalances;

impl Mappable for MessageBalances {
    type Key = Address;
    type OwnedKey = Self::Key;
    type Value = Amount;
    type OwnedValue = Self::Value;
}

impl TableWithBlueprint for MessageBalances {
    type Blueprint = Plain<Raw, Postcard>;
    type Column = super::Column;

    fn column() -> Self::Column {
        Self::Column::MessageBalances
    }
}
