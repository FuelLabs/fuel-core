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

pub type ItemAmount = u64;
pub type TotalBalanceAmount = u128;

double_key!(CoinBalancesKey, Address, address, AssetId, asset_id);
impl Distribution<CoinBalancesKey> for Standard {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> CoinBalancesKey {
        let mut bytes = [0u8; CoinBalancesKey::LEN];
        rng.fill_bytes(bytes.as_mut());
        CoinBalancesKey::from_array(bytes)
    }
}

impl core::fmt::Display for CoinBalancesKey {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "address={} asset_id={}", self.address(), self.asset_id())
    }
}

/// This table stores the balances of coins per owner and asset id.
pub struct CoinBalances;

impl Mappable for CoinBalances {
    type Key = CoinBalancesKey;
    type OwnedKey = Self::Key;
    type Value = TotalBalanceAmount;
    type OwnedValue = Self::Value;
}

impl TableWithBlueprint for CoinBalances {
    type Blueprint = Plain<Raw, Postcard>;
    type Column = super::Column;

    fn column() -> Self::Column {
        Self::Column::CoinBalances
    }
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct MessageBalance {
    pub retryable: TotalBalanceAmount,
    pub non_retryable: TotalBalanceAmount,
}

/// This table stores the balances of messages per owner.
pub struct MessageBalances;

impl Mappable for MessageBalances {
    type Key = Address;
    type OwnedKey = Self::Key;
    type Value = MessageBalance;
    type OwnedValue = Self::Value;
}

impl TableWithBlueprint for MessageBalances {
    type Blueprint = Plain<Raw, Postcard>;
    type Column = super::Column;

    fn column() -> Self::Column {
        Self::Column::MessageBalances
    }
}
