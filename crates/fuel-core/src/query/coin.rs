use crate::{
    graphql_api::service::Database,
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
    fuel_tx::UtxoId,
    fuel_types::Address,
};

pub struct CoinQueryContext<'a>(pub &'a Database);

impl<'a> CoinQueryContext<'a> {
    pub fn coin(&self, utxo_id: UtxoId) -> StorageResult<Coin> {
        let coin = self
            .0
            .as_ref()
            .storage::<Coins>()
            .get(&utxo_id)?
            .ok_or(not_found!(Coins))?
            .into_owned();

        Ok(coin.uncompress(utxo_id))
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
    ) -> impl Iterator<Item = StorageResult<Coin>> + '_ {
        self.owned_coins_ids(owner, start_coin, direction)
            .map(|res| res.and_then(|id| self.coin(id)))
    }
}
