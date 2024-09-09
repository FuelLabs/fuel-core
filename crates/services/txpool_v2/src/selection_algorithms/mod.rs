use fuel_core_types::services::txpool::PoolTransaction;

use crate::storage::Storage;

pub trait SelectionAlgorithm {
    type Storage: Storage;

    fn select(&mut self) -> Vec<Storage::StorageIndex>;

    fn new_executable_transactions(&mut self, transactions: Vec<Storage::StorageIndex>);
}