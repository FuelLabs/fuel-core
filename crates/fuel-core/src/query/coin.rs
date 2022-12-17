use fuel_core_storage::Result as StorageResult;

use fuel_core_types::{
    entities::coin::Coin,
    fuel_tx::UtxoId,
};

#[cfg_attr(test, mockall::automock)]
/// Trait that specifies all the data required by the output message query.
pub trait CoinQueryData {
    fn coin(&self, utxo_id: UtxoId) -> StorageResult<Option<Coin>>;
}
