//! Ports used by the relayer to access the outside world

use async_trait::async_trait;
use fuel_core_storage::Result as StorageResult;
use fuel_core_types::{
    blockchain::primitives::DaBlockHeight,
    entities::message::{
        CheckedMessage,
        Message,
    },
};

/// Manages state related to supported external chains.
#[async_trait]
pub trait RelayerDb: Send + Sync {
    /// Add bridge message to database. Messages are not revertible.
    async fn insert_message(
        &mut self,
        message: &CheckedMessage,
    ) -> StorageResult<Option<Message>>;

    /// set finalized da height that represent last block from da layer that got finalized.
    async fn set_finalized_da_height(
        &mut self,
        block: DaBlockHeight,
    ) -> StorageResult<()>;

    /// Assume it is always set as initialization of database.
    async fn get_finalized_da_height(&self) -> StorageResult<DaBlockHeight>;
}
