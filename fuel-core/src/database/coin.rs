use crate::{
    database::{
        Column,
        Database,
        KvStoreError,
    },
    model::Coin,
    multikey,
    state::{
        Error,
        IterDirection,
        UtxoIdAsRef,
    },
};
use fuel_chain_config::CoinConfig;
use fuel_core_interfaces::{
    common::{
        fuel_storage::{
            StorageInspect,
            StorageMutate,
        },
        fuel_tx::{
            Address,
            Bytes32,
            UtxoId,
        },
    },
    db::Coins,
};
use std::borrow::Cow;

impl StorageInspect<Coins> for Database {
    type Error = KvStoreError;

    fn get(&self, key: &UtxoId) -> Result<Option<Cow<Coin>>, KvStoreError> {
        self._get(key.as_ref(), Column::Coins).map_err(Into::into)
    }

    fn contains_key(&self, key: &UtxoId) -> Result<bool, KvStoreError> {
        self._contains_key(key.as_ref(), Column::Coins)
            .map_err(Into::into)
    }
}

impl StorageMutate<Coins> for Database {
    fn insert(
        &mut self,
        key: &UtxoId,
        value: &Coin,
    ) -> Result<Option<Coin>, KvStoreError> {
        let coin_by_owner = multikey!(&value.owner, Address, key, UtxoId);
        // insert primary record
        let insert = self._insert(key.as_ref(), Column::Coins, value)?;
        // insert secondary index by owner
        let _: Option<bool> =
            self._insert(coin_by_owner.as_ref(), Column::OwnedCoins, &true)?;
        Ok(insert)
    }

    fn remove(&mut self, key: &UtxoId) -> Result<Option<Coin>, KvStoreError> {
        let coin: Option<Coin> = self._remove(key.as_ref(), Column::Coins)?;

        // cleanup secondary index
        if let Some(coin) = &coin {
            let key = multikey!(&coin.owner, Address, key, UtxoId);
            let _: Option<bool> = self._remove(key.as_ref(), Column::OwnedCoins)?;
        }

        Ok(coin)
    }
}

impl Database {
    pub fn owned_coins_ids<'a>(
        &'a self,
        owner: &'a Address,
        start_coin: Option<UtxoId>,
        direction: Option<IterDirection>,
    ) -> impl Iterator<Item = Result<UtxoId, Error>> + 'a {
        self.iter_all::<Vec<u8>, bool>(
            Column::OwnedCoins,
            Some(owner.to_vec()),
            start_coin.map(|b| multikey!(owner, Address, &b, UtxoId).to_vec()),
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

    pub fn get_coin_config(&self) -> anyhow::Result<Option<Vec<CoinConfig>>> {
        let configs = self
            .iter_all::<Vec<u8>, Coin>(Column::Coins, None, None, None)
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
