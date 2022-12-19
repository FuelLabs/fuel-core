use super::get_sealed_block;
use crate::database::Database;
use fuel_core_p2p::ports::P2pDb;
use fuel_core_types::blockchain::{
    primitives::BlockHeight,
    SealedBlock,
};
use std::sync::Arc;

#[cfg(feature = "p2p")]
#[async_trait::async_trait]
impl P2pDb for Database {
    async fn get_sealed_block(&self, height: BlockHeight) -> Option<Arc<SealedBlock>> {
        get_sealed_block(self, height)
    }
}
