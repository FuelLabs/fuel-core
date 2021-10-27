use crate::{
    database::{
        columns::{self, OWNED_COINS},
        Database, KvStoreError,
    },
    model::coin::Coin,
    state::{Error, IterDirection},
};
use fuel_storage::Storage;
use fuel_tx::{Address, Bytes32};
use std::borrow::Cow;

fn owner_coin_id_key(owner: &Address, coin_id: &Bytes32) -> Vec<u8> {
    owner
        .as_ref()
        .iter()
        .chain(coin_id.as_ref().iter())
        .copied()
        .collect()
}

impl Storage<Bytes32, Coin> for Database {
    type Error = KvStoreError;

    fn insert(&mut self, key: &Bytes32, value: &Coin) -> Result<Option<Coin>, KvStoreError> {
        let coin_by_owner: Vec<u8> = owner_coin_id_key(&value.owner, key);
        // insert primary record
        let insert = Database::insert(self, key.as_ref().to_vec(), columns::COIN, value.clone())?;
        // insert secondary index by owner
        Database::insert(self, coin_by_owner, columns::OWNED_COINS, true)?;
        Ok(insert)
    }

    fn remove(&mut self, key: &Bytes32) -> Result<Option<Coin>, KvStoreError> {
        let coin: Option<Coin> = Database::remove(self, key.as_ref(), columns::COIN)?;

        // cleanup secondary index
        if let Some(coin) = &coin {
            let key = owner_coin_id_key(&coin.owner, key);
            let _: Option<bool> = Database::remove(self, key.as_slice(), columns::OWNED_COINS)?;
        }

        Ok(coin)
    }

    fn get(&self, key: &Bytes32) -> Result<Option<Cow<Coin>>, KvStoreError> {
        Database::get(self, key.as_ref(), columns::COIN).map_err(Into::into)
    }

    fn contains_key(&self, key: &Bytes32) -> Result<bool, KvStoreError> {
        Database::exists(self, key.as_ref(), columns::COIN).map_err(Into::into)
    }
}

impl Database {
    pub fn owned_coins(
        &self,
        owner: Address,
        start_coin: Option<Bytes32>,
        direction: Option<IterDirection>,
    ) -> impl Iterator<Item = Result<Bytes32, Error>> + '_ {
        self.iter_all::<Vec<u8>, bool>(
            OWNED_COINS,
            Some(owner.as_ref().to_vec()),
            start_coin.map(|b| owner_coin_id_key(&owner, &b)),
            direction,
        )
        // Safety: key is always 64 bytes
        .map(|res| res.map(|(key, _)| unsafe { Bytes32::from_slice_unchecked(&key[32..]) }))
    }
}
