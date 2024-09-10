use fuel_core_types::services::txpool::PoolTransaction;

use crate::{
    error::Error,
    storage::Storage,
};

pub mod ratio_tip_gas;

pub struct Constraints {
    pub max_gas: u64,
}

pub trait SelectionAlgorithm<S: Storage> {
    fn gather_best_txs(
        &mut self,
        constraints: Constraints,
        storage: &mut S,
    ) -> Result<Vec<PoolTransaction>, Error>;

    fn new_executable_transactions(
        &mut self,
        transactions_ids: Vec<S::StorageIndex>,
        storage: &S,
    ) -> Result<(), Error>;

    fn on_removed_transaction(
        &mut self,
        transaction: &PoolTransaction,
    ) -> Result<(), Error>;
}
