use crate::{
    config::chain_config::CoinConfig,
    database::{
        columns::{self, COIN, OWNED_COINS},
        Database, KvStoreError,
    },
    model::Coin,
    state::{Error, IterDirection},
};
use fuel_core_interfaces::common::{
    fuel_storage::Storage,
    fuel_tx::{Address, AssetId, Bytes32, UtxoId},
};
use fuel_core_interfaces::model::Coin as CoinModel;
use itertools::Itertools;
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

impl Storage<UtxoId, Coin> for Database {
    type Error = KvStoreError;

    fn insert(&mut self, key: &UtxoId, value: &Coin) -> Result<Option<Coin>, KvStoreError> {
        let coin_by_owner: Vec<u8> = owner_coin_id_key(&value.owner, key);
        // insert primary record
        let insert = Database::insert(self, utxo_id_to_bytes(key), columns::COIN, value.clone())?;
        // insert secondary index by owner
        Database::insert(self, coin_by_owner, columns::OWNED_COINS, true)?;
        Ok(insert)
    }

    fn remove(&mut self, key: &UtxoId) -> Result<Option<Coin>, KvStoreError> {
        let coin: Option<Coin> = Database::remove(self, &utxo_id_to_bytes(key), columns::COIN)?;

        // cleanup secondary index
        if let Some(coin) = &coin {
            let key = owner_coin_id_key(&coin.owner, key);
            let _: Option<bool> = Database::remove(self, key.as_slice(), columns::OWNED_COINS)?;
        }

        Ok(coin)
    }

    fn get(&self, key: &UtxoId) -> Result<Option<Cow<Coin>>, KvStoreError> {
        Database::get(self, &utxo_id_to_bytes(key), columns::COIN).map_err(Into::into)
    }

    fn contains_key(&self, key: &UtxoId) -> Result<bool, KvStoreError> {
        Database::exists(self, &utxo_id_to_bytes(key), columns::COIN).map_err(Into::into)
    }
}

impl Database {
    pub fn owned_coins(
        &self,
        owner: Address,
        start_coin: Option<UtxoId>,
        direction: Option<IterDirection>,
    ) -> impl Iterator<Item = Result<UtxoId, Error>> + '_ {
        self.iter_all::<Vec<u8>, bool>(
            OWNED_COINS,
            Some(owner.as_ref().to_vec()),
            start_coin.map(|b| owner_coin_id_key(&owner, &b)),
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

    // TODO: Optimize this by creating an index
    pub fn owned_coins_by_asset_id(
        &self,
        owner: Address,
        asset_id: AssetId,
        start_coin: Option<UtxoId>,
        direction: Option<IterDirection>,
    ) -> impl Iterator<Item = Result<UtxoId, Error>> + '_ {
        self.iter_all::<Vec<u8>, bool>(
            OWNED_COINS,
            Some(owner.as_ref().to_vec()),
            start_coin.map(|b| owner_coin_id_key(&owner, &b)),
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
        .filter_ok(move |id| {
            Storage::<UtxoId, Coin>::get(self, id)
                .unwrap()
                .unwrap()
                .asset_id
                == asset_id
        })
    }

    pub fn get_coin_config(&self) -> anyhow::Result<Option<Vec<CoinConfig>>> {
        let configs = self
            .iter_all::<Vec<u8>, CoinModel>(COIN, None, None, None)
            .map(|raw_coin| -> Result<CoinConfig, anyhow::Error> {
                let coin = raw_coin?;

                let byte_id = Bytes32::new(coin.0[..32].try_into()?);
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
            .collect::<Result<Vec<CoinConfig>, anyhow::Error>>()?;

        Ok(Some(configs))
    }
}
