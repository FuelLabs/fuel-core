use crate::database::{
    Column,
    Database,
};
use fuel_core_chain_config::CoinConfig;
use fuel_core_storage::{
    blueprint::plain::Plain,
    codec::{
        postcard::Postcard,
        primitive::utxo_id_to_bytes,
        raw::Raw,
    },
    iter::IterDirection,
    not_found,
    structured_storage::TableWithBlueprint,
    tables::Coins,
    Error as StorageError,
    Mappable,
    Result as StorageResult,
    StorageAsMut,
    StorageAsRef,
    StorageInspect,
    StorageMutate,
};
use fuel_core_txpool::types::TxId;
use fuel_core_types::{
    entities::coins::coin::CompressedCoin,
    fuel_tx::{
        Address,
        UtxoId,
    },
};
use std::borrow::Cow;

// TODO: Reuse `fuel_vm::storage::double_key` macro.
pub fn owner_coin_id_key(owner: &Address, coin_id: &UtxoId) -> OwnedCoinKey {
    let mut default = [0u8; Address::LEN + TxId::LEN + 1];
    default[0..Address::LEN].copy_from_slice(owner.as_ref());
    default[Address::LEN..].copy_from_slice(utxo_id_to_bytes(coin_id).as_ref());
    default
}

/// The storage table of owned coin ids. Maps addresses to owned coins.
pub struct OwnedCoins;
/// The storage key for owned coins: `Address ++ UtxoId`
pub type OwnedCoinKey = [u8; Address::LEN + TxId::LEN + 1];

impl Mappable for OwnedCoins {
    type Key = Self::OwnedKey;
    type OwnedKey = OwnedCoinKey;
    type Value = Self::OwnedValue;
    type OwnedValue = ();
}

impl TableWithBlueprint for OwnedCoins {
    type Blueprint = Plain<Raw, Postcard>;

    fn column() -> Column {
        Column::OwnedCoins
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn generate_key(rng: &mut impl rand::Rng) -> <OwnedCoins as Mappable>::Key {
        let mut bytes = [0u8; 65];
        rng.fill(bytes.as_mut());
        bytes
    }

    fuel_core_storage::basic_storage_tests!(
        OwnedCoins,
        [0u8; 65],
        <OwnedCoins as Mappable>::Value::default(),
        <OwnedCoins as Mappable>::Value::default(),
        generate_key
    );
}

impl StorageInspect<Coins> for Database {
    type Error = StorageError;

    fn get(&self, key: &UtxoId) -> Result<Option<Cow<CompressedCoin>>, Self::Error> {
        self.data.storage::<Coins>().get(key)
    }

    fn contains_key(&self, key: &UtxoId) -> Result<bool, Self::Error> {
        self.data.storage::<Coins>().contains_key(key)
    }
}

impl StorageMutate<Coins> for Database {
    fn insert(
        &mut self,
        key: &UtxoId,
        value: &CompressedCoin,
    ) -> Result<Option<CompressedCoin>, Self::Error> {
        let coin_by_owner = owner_coin_id_key(&value.owner, key);
        // insert primary record
        let insert = self.data.storage_as_mut::<Coins>().insert(key, value)?;
        // insert secondary index by owner
        self.storage_as_mut::<OwnedCoins>()
            .insert(&coin_by_owner, &())?;
        Ok(insert)
    }

    fn remove(&mut self, key: &UtxoId) -> Result<Option<CompressedCoin>, Self::Error> {
        let coin = self.data.storage_as_mut::<Coins>().remove(key)?;

        // cleanup secondary index
        if let Some(coin) = &coin {
            let key = owner_coin_id_key(&coin.owner, key);
            self.storage_as_mut::<OwnedCoins>().remove(&key)?;
        }

        Ok(coin)
    }
}

impl Database {
    pub fn owned_coins_ids(
        &self,
        owner: &Address,
        start_coin: Option<UtxoId>,
        direction: Option<IterDirection>,
    ) -> impl Iterator<Item = StorageResult<UtxoId>> + '_ {
        let start_coin = start_coin.map(|b| owner_coin_id_key(owner, &b));
        self.iter_all_filtered::<OwnedCoins, _>(
            Some(*owner), start_coin.as_ref(),
            direction,
        )
        // Safety: key is always 64 bytes
        .map(|res| {
            res.map(|(key, _)| {
                UtxoId::new(
                    TxId::try_from(&key[32..64]).expect("The slice has size 32"),
                    key[64],
                )
            })
        })
    }

    pub fn coin(&self, utxo_id: &UtxoId) -> StorageResult<CompressedCoin> {
        let coin = self
            .storage_as_ref::<Coins>()
            .get(utxo_id)?
            .ok_or(not_found!(Coins))?
            .into_owned();

        Ok(coin)
    }

    pub fn get_coin_config(&self) -> StorageResult<Option<Vec<CoinConfig>>> {
        let configs = self
            .iter_all::<Coins>(None)
            .map(|raw_coin| -> StorageResult<CoinConfig> {
                let (utxo_id, coin) = raw_coin?;

                Ok(CoinConfig {
                    tx_id: Some(*utxo_id.tx_id()),
                    output_index: Some(utxo_id.output_index()),
                    tx_pointer_block_height: Some(coin.tx_pointer.block_height()),
                    tx_pointer_tx_idx: Some(coin.tx_pointer.tx_index()),
                    maturity: Some(coin.maturity),
                    owner: coin.owner,
                    amount: coin.amount,
                    asset_id: coin.asset_id,
                })
            })
            .collect::<StorageResult<Vec<CoinConfig>>>()?;

        Ok(Some(configs))
    }
}
