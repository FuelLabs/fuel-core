use async_trait::async_trait;
use fuel_core_storage::{
    tables::Messages,
    Error as StorageError,
    StorageAsMut,
    StorageMutate,
};
use fuel_core_types::{
    blockchain::{
        primitives::{
            BlockHeight,
            DaBlockHeight,
        },
        SealedBlock,
    },
    entities::message::CheckedMessage,
};
use std::sync::Arc;

// Manages state related to supported external chains.
#[async_trait]
pub trait RelayerDb: StorageMutate<Messages, Error = StorageError> + Send + Sync {
    /// Add bridge message to database. Messages are not revertible.
    async fn insert_message(&mut self, message: &CheckedMessage) {
        let _ = self
            .storage::<Messages>()
            .insert(message.id(), message.as_ref());
    }

    /// current best block number
    async fn get_chain_height(&self) -> BlockHeight;

    async fn get_sealed_block(&self, height: BlockHeight) -> Option<Arc<SealedBlock>>;

    /// set finalized da height that represent last block from da layer that got finalized.
    async fn set_finalized_da_height(&self, block: DaBlockHeight);

    /// Assume it is always set as initialization of database.
    async fn get_finalized_da_height(&self) -> Option<DaBlockHeight>;

    /// Get the last fuel block height that was published to the da layer.
    async fn get_last_published_fuel_height(&self) -> Option<BlockHeight>;

    /// Set the last fuel block height that was published to the da layer.
    async fn set_last_published_fuel_height(&self, block_height: BlockHeight);
}
