//! Ports used by the relayer to access the outside world

use async_trait::async_trait;
use fuel_core_storage::Result as StorageResult;
use fuel_core_types::{
    blockchain::primitives::DaBlockHeight,
    entities::message::CheckedMessage,
};

/// Manages state related to supported external chains.
#[async_trait]
pub trait RelayerDb: Send + Sync {
    /// Add bridge messages to database. Messages are not revertible.
    fn insert_messages(
        &mut self,
        messages: &[(DaBlockHeight, CheckedMessage)],
    ) -> StorageResult<()>;

    /// set finalized da height that represent last block from da layer that got finalized.
    fn set_finalized_da_height(&mut self, block: DaBlockHeight) -> StorageResult<()>;

    /// Assume it is always set as initialization of database.
    fn get_finalized_da_height(&self) -> StorageResult<DaBlockHeight>;
}
