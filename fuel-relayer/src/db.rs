use async_trait::async_trait;
use fuel_core_interfaces::{
    common::fuel_storage::{
        StorageAsMut,
        StorageMutate,
    },
    model::{
        BlockHeight,
        CheckedMessage,
        DaBlockHeight,
        SealedFuelBlock,
    },
};
use fuel_database::{
    tables::Messages,
    Column,
    Database,
    KvStoreError,
};
use std::sync::Arc;

pub(crate) const FINALIZED_DA_HEIGHT_KEY: &[u8] = b"finalized_da_height";
pub(crate) const LAST_PUBLISHED_BLOCK_HEIGHT_KEY: &[u8] = b"last_publish_block_height";

/// Manages state related to supported external chains.
#[async_trait]
pub trait RelayerDb: StorageMutate<Messages, Error = KvStoreError> + Send + Sync {
    /// Add bridge message to database. Messages are not revertible.
    async fn insert_message(&mut self, message: &CheckedMessage);

    /// current best block number
    async fn get_chain_height(&self) -> BlockHeight;

    /// Returns the sealed block at the `height`.
    async fn get_sealed_block(&self, height: BlockHeight)
        -> Option<Arc<SealedFuelBlock>>;

    /// set finalized da height that represent last block from da layer that got finalized.
    async fn set_finalized_da_height(&self, block: DaBlockHeight);

    /// Assume it is always set as initialization of database.
    async fn get_finalized_da_height(&self) -> Option<DaBlockHeight>;

    /// Get the last fuel block height that was published to the da layer.
    async fn get_last_published_fuel_height(&self) -> Option<BlockHeight>;

    /// Set the last fuel block height that was published to the da layer.
    async fn set_last_published_fuel_height(&self, block_height: BlockHeight);
}

#[async_trait::async_trait]
impl RelayerDb for Database {
    async fn insert_message(&mut self, message: &CheckedMessage) {
        let _ = self
            .storage::<Messages>()
            .insert(message.id(), message.as_ref());
    }

    async fn get_chain_height(&self) -> BlockHeight {
        match self.get_block_height() {
            Ok(res) => {
                res.expect("get_block_height value should be always present and set")
            }
            Err(err) => {
                panic!("get_block_height database corruption, err:{:?}", err);
            }
        }
    }

    async fn get_sealed_block(
        &self,
        height: BlockHeight,
    ) -> Option<Arc<SealedFuelBlock>> {
        let block_id = self
            .get_block_id(height)
            .unwrap_or_else(|_| panic!("nonexistent block height {}", height))?;

        self.get_sealed_block(&block_id)
            .expect("expected to find sealed block")
            .map(Arc::new)
    }

    async fn set_finalized_da_height(&self, block: DaBlockHeight) {
        let _: Option<BlockHeight> = self
            .insert(FINALIZED_DA_HEIGHT_KEY, Column::Metadata, block)
            .unwrap_or_else(|err| {
                panic!("set_finalized_da_height should always succeed: {:?}", err);
            });
    }

    async fn get_finalized_da_height(&self) -> Option<DaBlockHeight> {
        match self.get(FINALIZED_DA_HEIGHT_KEY, Column::Metadata) {
            Ok(res) => res,
            Err(err) => {
                panic!("get_finalized_da_height database corruption, err:{:?}", err);
            }
        }
    }

    async fn get_last_published_fuel_height(&self) -> Option<BlockHeight> {
        match self.get(LAST_PUBLISHED_BLOCK_HEIGHT_KEY, Column::Metadata) {
            Ok(res) => res,
            Err(err) => {
                panic!(
                    "set_last_committed_finalized_fuel_height database corruption, err:{:?}",
                    err
                );
            }
        }
    }

    async fn set_last_published_fuel_height(&self, block_height: BlockHeight) {
        if let Err(err) = self.insert::<_, _, BlockHeight>(
            LAST_PUBLISHED_BLOCK_HEIGHT_KEY,
            Column::Metadata,
            block_height,
        ) {
            panic!(
                "set_pending_committed_fuel_height should always succeed: {:?}",
                err
            );
        }
    }
}
