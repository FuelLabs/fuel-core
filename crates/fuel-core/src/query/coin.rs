use crate::{
    graphql_api::service::DatabaseTemp,
    state::IterDirection,
};
use fuel_core_storage::{
    tables::Coins,
    Result as StorageResult,
    StorageAsRef,
};
use fuel_core_types::{
    entities::coin::Coin,
    fuel_tx::{
        UtxoId,
        UtxoId as UtxoIdModel,
    },
    fuel_types::Address,
};

pub struct CoinQueryContext<'a>(pub &'a DatabaseTemp);

impl CoinQueryContext<'_> {
    pub fn coin(
        &self,
        utxo_id: UtxoIdModel,
    ) -> StorageResult<Option<fuel_core_types::entities::coin::Coin>> {
        let db = self.0;

        let block = db
            .storage::<Coins>()
            .get(&utxo_id)?
            .map(|coin| coin.clone().into_owned());

        Ok(block)
    }

    pub fn owned_coins_ids(
        &self,
        owner: &Address,
        start_coin: Option<UtxoId>,
        direction: IterDirection,
    ) -> impl Iterator<Item = StorageResult<UtxoId>> + '_ {
        self.0.owned_coins_ids(owner, start_coin, direction)
    }
}

// FilterMap<Map<IntoIter<UtxoId>, |UtxoId| -> Result<(UtxoId, Coin), Error>>, |Result<(UtxoId, Coin), Error>| -> Option<Result<(UtxoId, Coin), Error>>>
