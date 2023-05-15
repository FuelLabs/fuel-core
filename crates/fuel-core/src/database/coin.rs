use crate::database::{
    storage::DatabaseColumn,
    Column,
    Database,
    Error as DatabaseError,
    Result as DatabaseResult,
};
use fuel_core_chain_config::CoinConfig;
use fuel_core_storage::{
    iter::IterDirection,
    not_found,
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
        Bytes32,
        UtxoId,
    },
};

use super::DbValue;

// TODO: Reuse `fuel_vm::storage::double_key` macro.
pub fn owner_coin_id_key(owner: &Address, coin_id: &UtxoId) -> OwnedCoinKey {
    let mut default = [0u8; Address::LEN + TxId::LEN + 1];
    default[0..Address::LEN].copy_from_slice(owner.as_ref());
    default[Address::LEN..].copy_from_slice(utxo_id_to_bytes(coin_id).as_ref());
    default
}

fn utxo_id_to_bytes(utxo_id: &UtxoId) -> [u8; TxId::LEN + 1] {
    let mut default = [0; TxId::LEN + 1];
    default[0..TxId::LEN].copy_from_slice(utxo_id.tx_id().as_ref());
    default[TxId::LEN] = utxo_id.output_index();
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
    type OwnedValue = bool;
}

impl DatabaseColumn for OwnedCoins {
    fn column() -> Column {
        Column::OwnedCoins
    }
}

impl StorageInspect<Coins> for Database {
    type Error = StorageError;

    fn get(&self, key: &UtxoId) -> Result<Option<CompressedCoin>, Self::Error> {
        let value = Database::get(self, &utxo_id_to_bytes(key), Column::Coins)?;
        Ok(value.map(|b| b.owned()))
    }

    fn contains_key(&self, key: &UtxoId) -> Result<bool, Self::Error> {
        Ok(Database::contains_key(
            self,
            &utxo_id_to_bytes(key),
            Column::Coins,
        )?)
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
        let insert = Database::insert(self, utxo_id_to_bytes(key), Column::Coins, value)?;
        // insert secondary index by owner
        self.storage_as_mut::<OwnedCoins>()
            .insert(&coin_by_owner, &true)?;
        Ok(insert.map(|b| b.owned()))
    }

    fn remove(&mut self, key: &UtxoId) -> Result<Option<CompressedCoin>, Self::Error> {
        let coin: Option<DbValue<CompressedCoin>> =
            Database::remove(self, &utxo_id_to_bytes(key), Column::Coins)?;
        let coin = coin.map(|b| b.owned());

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
    ) -> impl Iterator<Item = DatabaseResult<UtxoId>> + '_ {
        self.iter_all_filtered::<Vec<u8>, bool, _, _>(
            Column::OwnedCoins,
            Some(*owner),
            start_coin.map(|b| owner_coin_id_key(owner, &b)),
            direction,
        )
        // Safety: key is always 64 bytes
        .map(|res| {
            res.map(|row| {
                UtxoId::new(
                    TxId::try_from(&row.key[32..64]).expect("The slice has size 32"),
                    row.key[64],
                )
            })
        })
    }

    pub fn coin(&self, utxo_id: &UtxoId) -> StorageResult<CompressedCoin> {
        self.storage_as_ref::<Coins>()
            .get(utxo_id)?
            .ok_or(not_found!(Coins))
    }

    pub fn get_coin_config(&self) -> DatabaseResult<Option<Vec<CoinConfig>>> {
        let configs = self
            .iter_all::<Vec<u8>, CompressedCoin>(Column::Coins, None)
            .map(|raw_coin| -> DatabaseResult<CoinConfig> {
                let row = raw_coin?;

                let byte_id =
                    Bytes32::new(row.key[..32].try_into().map_err(DatabaseError::from)?);
                let output_index = row.key[32];

                let value = row.value.owned();

                Ok(CoinConfig {
                    tx_id: Some(byte_id),
                    output_index: Some(output_index),
                    tx_pointer_block_height: Some(value.tx_pointer.block_height()),
                    tx_pointer_tx_idx: Some(value.tx_pointer.tx_index()),
                    maturity: Some(value.maturity),
                    owner: value.owner,
                    amount: value.amount,
                    asset_id: value.asset_id,
                })
            })
            .collect::<DatabaseResult<Vec<CoinConfig>>>()?;

        Ok(Some(configs))
    }
}
