//! Ports used by the relayer to access the outside world

use async_trait::async_trait;
use fuel_core_storage::Result as StorageResult;
use fuel_core_types::{
    blockchain::primitives::DaBlockHeight,
    services::relayer::Event,
};

#[cfg(test)]
mod tests;

/// Manages state related to supported external chains.
#[async_trait]
pub trait RelayerDb: Send + Sync {
    /// Add bridge events to database. Events are not revertible.
    /// Must only set a new da height if it is greater than the current.
    fn insert_events(
        &mut self,
        da_height: &DaBlockHeight,
        events: &[Event],
    ) -> StorageResult<()>;

    /// Get finalized da height that represent last block from da layer that got finalized.
    /// Panics if height is not set as of initialization of database.
    fn get_finalized_da_height(&self) -> Option<DaBlockHeight>;
}

/// The trait that should be implemented by the database transaction returned by the database.
#[cfg_attr(test, mockall::automock)]
pub trait DatabaseTransaction {
    /// Commits the changes to the underlying storage.
    fn commit(self) -> StorageResult<()>;
}

/// The trait indicates that the type supports storage transactions.
pub trait Transactional {
    /// The type of the storage transaction;
    type Transaction<'a>: DatabaseTransaction
    where
        Self: 'a;

    /// Returns the storage transaction.
    fn transaction(&mut self) -> Self::Transaction<'_>;

    /// Returns the latest da block height.
    fn latest_da_height(&self) -> Option<DaBlockHeight>;
}
