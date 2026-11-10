//! Ports used by the relayer to access the outside world

use fuel_core_storage::Result as StorageResult;
use fuel_core_types::{
    blockchain::primitives::DaBlockHeight,
    services::relayer::Event,
};

#[cfg(test)]
mod tests;

/// Manages state related to supported external chains.
pub trait RelayerDb: Send + Sync {
    /// Add bridge events to database. Events are not revertible.
    /// Must only set a new da height if it is greater than the current.
    fn insert_events(
        &mut self,
        da_height: &DaBlockHeight,
        events: &[Event],
    ) -> StorageResult<()>;

    /// Commit verified Ethereum evidence atomically with events at the next DA height.
    fn insert_fcr_events(
        &mut self,
        height: &DaBlockHeight,
        events: &[Event],
        evidence: &crate::fcr::BlockEvidence,
    ) -> StorageResult<()> {
        let _ = (height, events, evidence);
        Err(anyhow::anyhow!("Database does not support FCR evidence").into())
    }

    /// Read evidence for an imported height.
    fn fcr_evidence(
        &self,
        height: &DaBlockHeight,
    ) -> StorageResult<Option<crate::fcr::BlockEvidence>> {
        let _ = height;
        Ok(None)
    }

    /// Whether this database carries the irreversible FCR activation marker.
    fn fcr_activated(&self) -> StorageResult<bool> {
        Ok(false)
    }

    /// Preserve the current DA height while activating the FCR database marker.
    fn activate_fcr(&mut self, anchor: DaBlockHeight) -> StorageResult<()> {
        {
            let _ = anchor;
            Err(anyhow::anyhow!("Database does not support FCR activation").into())
        }
    }

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

    /// Read verified Ethereum evidence without opening a height-advancing transaction.
    fn fcr_evidence(
        &self,
        height: &DaBlockHeight,
    ) -> StorageResult<Option<crate::fcr::BlockEvidence>> {
        let _ = height;
        Ok(None)
    }

    /// Read the one-way activation marker.
    fn fcr_activated(&self) -> StorageResult<bool> {
        Ok(false)
    }

    /// Migrate the database metadata without inventing a DA height.
    fn activate_fcr(&mut self, anchor: DaBlockHeight) -> StorageResult<()> {
        {
            let _ = anchor;
            Err(anyhow::anyhow!("Database does not support FCR activation").into())
        }
    }

    /// Returns the latest da block height.
    fn latest_da_height(&self) -> Option<DaBlockHeight>;
}
