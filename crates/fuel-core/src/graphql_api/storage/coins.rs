use fuel_core_storage::{
    blueprint::plain::Plain,
    codec::{
        postcard::Postcard,
        primitive::utxo_id_to_bytes,
        raw::Raw,
    },
    structured_storage::TableWithBlueprint,
    Mappable,
};
use fuel_core_types::{
    entities::coins::coin::Coin,
    fuel_tx::{
        Address,
        AssetId,
        TxId,
        UtxoId,
    },
};

use super::balances::ItemAmount;

// TODO: Reuse `fuel_vm::storage::double_key` macro.
pub fn owner_coin_id_key(owner: &Address, coin_id: &UtxoId) -> OwnedCoinKey {
    let mut default = [0u8; Address::LEN + TxId::LEN + 2];
    default[0..Address::LEN].copy_from_slice(owner.as_ref());
    let utxo_id_bytes: [u8; TxId::LEN + 2] = utxo_id_to_bytes(coin_id);
    default[Address::LEN..].copy_from_slice(utxo_id_bytes.as_ref());
    default
}

/// The storage table for the index of coins to spend.

pub struct CoinsToSpendIndex;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CoinsToSpendIndexKey([u8; CoinsToSpendIndexKey::LEN]);

impl Default for CoinsToSpendIndexKey {
    fn default() -> Self {
        Self([0u8; CoinsToSpendIndexKey::LEN])
    }
}

impl CoinsToSpendIndexKey {
    const LEN: usize =
        Address::LEN + AssetId::LEN + u64::BITS as usize / 8 + TxId::LEN + 2;

    pub fn new(coin: &Coin) -> Self {
        let address_bytes = coin.owner.as_ref();
        let asset_id_bytes = coin.asset_id.as_ref();
        let amount_bytes = coin.amount.to_be_bytes();
        let utxo_id_bytes = utxo_id_to_bytes(&coin.utxo_id);

        let mut arr = [0; CoinsToSpendIndexKey::LEN];
        let mut offset = 0;
        arr[offset..offset + Address::LEN].copy_from_slice(address_bytes);
        offset += Address::LEN;
        arr[offset..offset + AssetId::LEN].copy_from_slice(asset_id_bytes);
        offset += AssetId::LEN;
        arr[offset..offset + u64::BITS as usize / 8].copy_from_slice(&amount_bytes);
        offset += u64::BITS as usize / 8;
        arr[offset..].copy_from_slice(&utxo_id_bytes);
        Self(arr)
    }

    pub fn from_slice(slice: &[u8]) -> Result<Self, core::array::TryFromSliceError> {
        Ok(Self(slice.try_into()?))
    }

    // TODO[RC]: Test this
    pub fn utxo_id(&self) -> UtxoId {
        let mut offset = 0;
        offset += Address::LEN;
        offset += AssetId::LEN;
        offset += ItemAmount::BITS as usize / 8;

        let txid_start = 0 + offset;
        let txid_end = txid_start + TxId::LEN;

        let output_index_start = txid_end;

        let tx_id: [u8; TxId::LEN] = self.0[txid_start..txid_end]
            .try_into()
            .expect("TODO[RC]: Fix this");
        let output_index = u16::from_be_bytes(
            self.0[output_index_start..]
                .try_into()
                .expect("TODO[RC]: Fix this"),
        );
        UtxoId::new(TxId::from(tx_id), output_index)
    }
}

impl TryFrom<&[u8]> for CoinsToSpendIndexKey {
    type Error = core::array::TryFromSliceError;
    fn try_from(slice: &[u8]) -> Result<Self, Self::Error> {
        CoinsToSpendIndexKey::from_slice(slice)
    }
}

impl AsRef<[u8]> for CoinsToSpendIndexKey {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl Mappable for CoinsToSpendIndex {
    type Key = Self::OwnedKey;
    type OwnedKey = CoinsToSpendIndexKey;
    type Value = Self::OwnedValue;
    type OwnedValue = ();
}

impl TableWithBlueprint for CoinsToSpendIndex {
    type Blueprint = Plain<Raw, Postcard>;
    type Column = super::Column;

    fn column() -> Self::Column {
        Self::Column::CoinsToSpend
    }
}

/// The storage table of owned coin ids. Maps addresses to owned coins.
pub struct OwnedCoins;
/// The storage key for owned coins: `Address ++ UtxoId`
pub type OwnedCoinKey = [u8; Address::LEN + TxId::LEN + 2];

impl Mappable for OwnedCoins {
    type Key = Self::OwnedKey;
    type OwnedKey = OwnedCoinKey;
    type Value = Self::OwnedValue;
    type OwnedValue = ();
}

impl TableWithBlueprint for OwnedCoins {
    type Blueprint = Plain<Raw, Postcard>;
    type Column = super::Column;

    fn column() -> Self::Column {
        Self::Column::OwnedCoins
    }
}

#[cfg(test)]
mod test {
    use super::*;

    impl rand::distributions::Distribution<CoinsToSpendIndexKey>
        for rand::distributions::Standard
    {
        fn sample<R: rand::Rng + ?Sized>(&self, rng: &mut R) -> CoinsToSpendIndexKey {
            let mut bytes = [0u8; CoinsToSpendIndexKey::LEN];
            rng.fill_bytes(bytes.as_mut());
            CoinsToSpendIndexKey(bytes)
        }
    }

    fn generate_key(rng: &mut impl rand::Rng) -> <OwnedCoins as Mappable>::Key {
        let mut bytes = [0u8; 66];
        rng.fill(bytes.as_mut());
        bytes
    }

    fuel_core_storage::basic_storage_tests!(
        OwnedCoins,
        [0u8; 66],
        <OwnedCoins as Mappable>::Value::default(),
        <OwnedCoins as Mappable>::Value::default(),
        generate_key
    );

    fuel_core_storage::basic_storage_tests!(
        CoinsToSpendIndex,
        <CoinsToSpendIndex as Mappable>::Key::default(),
        <CoinsToSpendIndex as Mappable>::Value::default()
    );
}
