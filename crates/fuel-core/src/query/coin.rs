use crate::database::Database;
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
};

#[cfg_attr(test, mockall::automock)]
/// Trait that specifies all the data required by the output message query.
pub trait CoinQueryData {
    fn coin(&self, utxo_id: UtxoId) -> StorageResult<Option<Coin>>;
}

pub struct CoinQueryContext<'a>(pub &'a Database);

impl CoinQueryData for CoinQueryContext<'_> {
    fn coin(
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
}

// FilterMap<Map<IntoIter<UtxoId>, |UtxoId| -> Result<(UtxoId, Coin), Error>>, |Result<(UtxoId, Coin), Error>| -> Option<Result<(UtxoId, Coin), Error>>>
