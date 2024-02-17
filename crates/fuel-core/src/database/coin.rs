use crate::{
    database::{
        database_description::off_chain::OffChain,
        Database,
    },
    fuel_core_graphql_api::storage::coins::{
        owner_coin_id_key,
        OwnedCoins,
    },
};
use fuel_core_chain_config::CoinConfig;
use fuel_core_storage::{
    iter::IterDirection,
    not_found,
    tables::Coins,
    Result as StorageResult,
    StorageAsRef,
};
use fuel_core_txpool::types::TxId;
use fuel_core_types::{
    entities::coins::coin::CompressedCoin,
    fuel_tx::{
        Address,
        UtxoId,
    },
};

impl Database<OffChain> {
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
}

impl Database {
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
                    tx_pointer_block_height: Some(coin.tx_pointer().block_height()),
                    tx_pointer_tx_idx: Some(coin.tx_pointer().tx_index()),
                    maturity: Some(*coin.maturity()),
                    owner: *coin.owner(),
                    amount: *coin.amount(),
                    asset_id: *coin.asset_id(),
                })
            })
            .collect::<StorageResult<Vec<CoinConfig>>>()?;

        Ok(Some(configs))
    }
}
