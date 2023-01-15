use crate::{
    database::{
        Column,
        Database,
        Error as DatabaseError,
        Result as DatabaseResult,
    },
    state::IterDirection,
};
use fuel_core_chain_config::CoinConfig;
use fuel_core_storage::{
    tables::Coins,
    Error as StorageError,
    StorageInspect,
    StorageMutate,
};
use fuel_core_types::{
    entities::coin::{
        CoinStatus,
        CompressedCoin,
    },
    fuel_tx::{
        Address,
        Bytes32,
        UtxoId,
    },
};
use std::borrow::Cow;

fn owner_coin_id_key(owner: &Address, coin_id: &UtxoId) -> Vec<u8> {
    owner
        .as_ref()
        .iter()
        .chain(utxo_id_to_bytes(coin_id).iter())
        .copied()
        .collect()
}

// 32 Bytes for Tx_id + 1 byte for output_index
const SIZE_OF_UTXO_ID: usize = 264;

fn utxo_id_to_bytes(utxo_id: &UtxoId) -> Vec<u8> {
    let mut out = Vec::with_capacity(SIZE_OF_UTXO_ID);
    out.extend(utxo_id.tx_id().as_ref().iter());
    out.push(utxo_id.output_index());
    out
}

impl StorageInspect<Coins> for Database {
    type Error = StorageError;

    fn get(&self, key: &UtxoId) -> Result<Option<Cow<CompressedCoin>>, Self::Error> {
        Database::get(self, &utxo_id_to_bytes(key), Column::Coins).map_err(Into::into)
    }

    fn contains_key(&self, key: &UtxoId) -> Result<bool, Self::Error> {
        Database::exists(self, &utxo_id_to_bytes(key), Column::Coins).map_err(Into::into)
    }
}

impl StorageMutate<Coins> for Database {
    fn insert(
        &mut self,
        key: &UtxoId,
        value: &CompressedCoin,
    ) -> Result<Option<CompressedCoin>, Self::Error> {
        let coin_by_owner: Vec<u8> = owner_coin_id_key(&value.owner, key);
        // insert primary record
        let insert = Database::insert(self, utxo_id_to_bytes(key), Column::Coins, value)?;
        // insert secondary index by owner
        let _: Option<bool> =
            Database::insert(self, coin_by_owner, Column::OwnedCoins, true)?;
        Ok(insert)
    }

    fn remove(&mut self, key: &UtxoId) -> Result<Option<CompressedCoin>, Self::Error> {
        let coin: Option<CompressedCoin> =
            Database::remove(self, &utxo_id_to_bytes(key), Column::Coins)?;

        // cleanup secondary index
        if let Some(coin) = &coin {
            let key = owner_coin_id_key(&coin.owner, key);
            let _: Option<bool> =
                Database::remove(self, key.as_slice(), Column::OwnedCoins)?;
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
        self.iter_all::<Vec<u8>, bool>(
            Column::OwnedCoins,
            Some(owner.as_ref().to_vec()),
            start_coin.map(|b| owner_coin_id_key(owner, &b)),
            direction,
        )
        // Safety: key is always 64 bytes
        .map(|res| {
            res.map(|(key, _)| {
                UtxoId::new(
                    unsafe { Bytes32::from_slice_unchecked(&key[32..64]) },
                    key[64],
                )
            })
        })
    }

    pub fn get_coin_config(&self) -> DatabaseResult<Option<Vec<CoinConfig>>> {
        let configs = self
            .iter_all::<Vec<u8>, CompressedCoin>(Column::Coins, None, None, None)
            .filter_map(|coin| {
                // Return only unspent coins
                if let Ok(coin) = coin {
                    if coin.1.status == CoinStatus::Unspent {
                        Some(Ok(coin))
                    } else {
                        None
                    }
                } else {
                    Some(coin)
                }
            })
            .map(|raw_coin| -> DatabaseResult<CoinConfig> {
                let coin = raw_coin?;

                let byte_id =
                    Bytes32::new(coin.0[..32].try_into().map_err(DatabaseError::from)?);
                let output_index = coin.0[32];

                Ok(CoinConfig {
                    tx_id: Some(byte_id),
                    output_index: Some(output_index.into()),
                    block_created: Some(coin.1.block_created),
                    maturity: Some(coin.1.maturity),
                    owner: coin.1.owner,
                    amount: coin.1.amount,
                    asset_id: coin.1.asset_id,
                })
            })
            .collect::<DatabaseResult<Vec<CoinConfig>>>()?;

        Ok(Some(configs))
    }
}
