use crate::fuel_core_graphql_api::database::ReadView;
use fuel_core_storage::{
    iter::{
        BoxedIter,
        IntoBoxedIter,
        IterDirection,
    },
    not_found,
    tables::Coins,
    Result as StorageResult,
    StorageAsRef,
};
use fuel_core_types::{
    entities::coins::coin::Coin,
    fuel_tx::UtxoId,
    fuel_types::Address,
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

    pub fn owned_coins(
        &self,
        owner: &Address,
        start_coin: Option<UtxoId>,
        direction: IterDirection,
    ) -> BoxedIter<StorageResult<Coin>> {
        self.owned_coins_ids(owner, start_coin, direction)
            .map(|res| res.and_then(|id| self.coin(id)))
            .into_boxed()
    }
}
