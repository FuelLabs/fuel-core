use crate::database::{
    metadata,
    Column,
    Database,
};
use fuel_core_relayer::ports::RelayerDb;
use fuel_core_types::blockchain::{
    primitives::{
        BlockHeight,
        DaBlockHeight,
    },
    SealedBlock,
};
use std::sync::Arc;

// TODO: Return `Result` instead of panics
#[async_trait::async_trait]
impl RelayerDb for Database {
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

    async fn get_sealed_block(&self, height: BlockHeight) -> Option<Arc<SealedBlock>> {
        // TODO: Return an error otherwise it will fail with panic in runtime.
        let block_id = self
            .get_block_id(height)
            .unwrap_or_else(|_| panic!("nonexistent block height {}", height))?;

        self.get_sealed_block(&block_id)
            .expect("expected to find sealed block")
            .map(Arc::new)
    }

    async fn set_finalized_da_height(&self, block: DaBlockHeight) {
        let _: Option<BlockHeight> = self
            .insert(metadata::FINALIZED_DA_HEIGHT_KEY, Column::Metadata, block)
            .unwrap_or_else(|err| {
                panic!("set_finalized_da_height should always succeed: {:?}", err);
            });
    }

    async fn get_finalized_da_height(&self) -> Option<DaBlockHeight> {
        match self.get(metadata::FINALIZED_DA_HEIGHT_KEY, Column::Metadata) {
            Ok(res) => res,
            Err(err) => {
                panic!("get_finalized_da_height database corruption, err:{:?}", err);
            }
        }
    }

    async fn get_last_published_fuel_height(&self) -> Option<BlockHeight> {
        match self.get(metadata::LAST_PUBLISHED_BLOCK_HEIGHT_KEY, Column::Metadata) {
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
            metadata::LAST_PUBLISHED_BLOCK_HEIGHT_KEY,
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
