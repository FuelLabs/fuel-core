use fuel_core_types::services::txpool::PoolTransaction;
use crate::error::Error;

pub trait Storage {
    type StorageIndex;

    fn store_transactions(&mut self, transaction: Vec<PoolTransaction>) -> Result<Self::StorageIndex, Error>;

    fn get_dependents(&self, index: Self::StorageIndex) -> Result<Vec<Self::StorageIndex>, Error>;
    
    fn remove_transactions(&mut self, index: Vec<Self::StorageIndex>) -> Result<(), Error>;
}