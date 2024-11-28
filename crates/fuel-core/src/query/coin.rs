use crate::{
    coins_query::CoinsQueryError,
    fuel_core_graphql_api::database::ReadView,
    graphql_api::indexation::coins_to_spend::IndexedCoinType,
};
use fuel_core_storage::{
    iter::IterDirection,
    not_found,
    tables::Coins,
    Error as StorageError,
    Result as StorageResult,
    StorageAsRef,
};
use fuel_core_types::{
    entities::coins::{
        coin::Coin,
        CoinType,
    },
    fuel_tx::{
        AssetId,
        UtxoId,
    },
    fuel_types::Address,
};
use futures::{
    Stream,
    StreamExt,
    TryStreamExt,
};

impl ReadView {
    pub fn coin(&self, utxo_id: UtxoId) -> StorageResult<Coin> {
        let coin = self
            .on_chain
            .as_ref()
            .storage::<Coins>()
            .get(&utxo_id)?
            .ok_or(not_found!(Coins))?
            .into_owned();

        Ok(coin.uncompress(utxo_id))
    }

    pub async fn coins(
        &self,
        utxo_ids: Vec<UtxoId>,
    ) -> impl Iterator<Item = StorageResult<Coin>> + '_ {
        // TODO: Use multiget when it's implemented.
        //  https://github.com/FuelLabs/fuel-core/issues/2344
        let coins = utxo_ids.into_iter().map(|id| self.coin(id));
        // Give a chance to other tasks to run.
        tokio::task::yield_now().await;
        coins
    }

    pub fn owned_coins(
        &self,
        owner: &Address,
        start_coin: Option<UtxoId>,
        direction: IterDirection,
    ) -> impl Stream<Item = StorageResult<Coin>> + '_ {
        self.owned_coins_ids(owner, start_coin, direction)
            .chunks(self.batch_size)
            .map(|chunk| {
                use itertools::Itertools;

                let chunk = chunk.into_iter().try_collect::<_, Vec<_>, _>()?;
                Ok::<_, StorageError>(chunk)
            })
            .try_filter_map(move |chunk| async move {
                let chunk = self.coins(chunk).await;
                Ok(Some(futures::stream::iter(chunk)))
            })
            .try_flatten()
    }

    pub fn coins_to_spend(
        &self,
        owner: &Address,
        asset_id: &AssetId,
        target_amount: u64,
        max_coins: u32,
    ) -> Result<Vec<(Vec<u8>, IndexedCoinType)>, CoinsQueryError> {
        Ok(self
            .off_chain
            .coins_to_spend(owner, asset_id, target_amount, max_coins)?)
    }
}
