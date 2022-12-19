use std::sync::Arc;

use crate::database::Database;
use fuel_core_p2p::ports::P2pDb;
use fuel_core_types::blockchain::{
    primitives::BlockHeight,
    SealedBlock,
};

#[cfg(feature = "p2p")]
#[async_trait::async_trait]
impl P2pDb for Database {
    async fn get_sealed_block(
        &self,
        block_height: BlockHeight,
    ) -> Option<Arc<SealedBlock>> {
        // todo: current signatures do not match?
        todo!()
    }
}
