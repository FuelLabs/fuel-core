use std::{
    collections::HashSet,
    fmt::Debug,
    time::Instant,
};

use crate::{
    collision_manager::CollisionReason,
    error::Error,
    ports::TxPoolDb,
};
use fuel_core_types::services::txpool::PoolTransaction;

pub mod graph;

#[derive(Debug)]
pub struct StorageData {
    pub transaction: PoolTransaction,
    /// The cumulative tip of a transaction and all of its children.
    pub cumulative_tip: u64,
    /// The cumulative gas of a transaction and all of its children.
    pub cumulative_gas: u64,
    /// Number of dependents
    pub number_txs_in_chain: u64,
    /// Submitted time
    pub submitted_time: Instant,
}

pub type RemovedTransactions = Vec<PoolTransaction>;

pub trait Storage {
    type StorageIndex: Copy + Debug;

    fn store_transaction(
        &mut self,
        transaction: PoolTransaction,
        dependencies: Vec<Self::StorageIndex>,
        collided_transactions: Vec<Self::StorageIndex>,
    ) -> Result<(Self::StorageIndex, RemovedTransactions), Error>;

    fn get(&self, index: &Self::StorageIndex) -> Result<&StorageData, Error>;

    fn get_dependencies(
        &self,
        index: Self::StorageIndex,
    ) -> Result<Vec<Self::StorageIndex>, Error>;

    fn get_dependents(
        &self,
        index: Self::StorageIndex,
    ) -> Result<Vec<Self::StorageIndex>, Error>;

    fn collect_dependencies_transactions(
        &self,
        transaction: &PoolTransaction,
        collisions: HashSet<CollisionReason>,
        db: &impl TxPoolDb,
        utxo_validation: bool,
    ) -> Result<Vec<Self::StorageIndex>, Error>;

    fn remove_transaction_and_dependents(
        &mut self,
        index: Self::StorageIndex,
    ) -> Result<(), Error>;

    fn remove_transaction(
        &mut self,
        index: Self::StorageIndex,
    ) -> Result<StorageData, Error>;

    fn count(&self) -> u64;
}
