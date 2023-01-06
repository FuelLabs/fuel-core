use crate::{
    graphql_api::service::DatabaseTemp,
    state::IterDirection,
};
use fuel_core_storage::{
    not_found,
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

impl<'a> CoinQueryContext<'a> {
    pub fn coin(&self, utxo_id: &UtxoId) -> StorageResult<Coin> {
        let coin = self
            .0
            .as_ref()
            .storage::<Coins>()
            .get(utxo_id)?
            .ok_or(not_found!(Coins))?
            .into_owned();

        Ok(coin)
    }

    pub fn owned_coins_ids(
        &self,
        owner: &Address,
        start_coin: Option<UtxoId>,
        direction: IterDirection,
    ) -> impl Iterator<Item = StorageResult<UtxoId>> + 'a {
        self.0.owned_coins_ids(owner, start_coin, direction)
    }

    pub fn owned_coins(
        &self,
        owner: &Address,
        start_coin: Option<UtxoId>,
        direction: IterDirection,
    ) -> impl Iterator<Item = StorageResult<(UtxoId, Coin)>> + '_ {
        self.owned_coins_ids(owner, start_coin, direction)
            .map(|res| res.and_then(|id| Ok((id, self.coin(&id)?))))
    }
}

// FilterMap<Map<IntoIter<UtxoId>, |UtxoId| -> Result<(UtxoId, Coin), Error>>, |Result<(UtxoId, Coin), Error>| -> Option<Result<(UtxoId, Coin), Error>>>
