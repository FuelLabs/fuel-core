use fuel_core_services::SourcePeer;
use fuel_core_types::{
    blockchain::{
        primitives::{
            BlockHeight,
            BlockId,
        },
        SealedBlock,
        SealedBlockHeader,
    },
    fuel_tx::Transaction,
    services::executor::ExecutionResult,
};

use futures::stream::BoxStream;

#[cfg(test)]
use mockall::automock;

#[cfg_attr(test, automock)]
#[async_trait::async_trait]
pub trait PeerToPeer {
    fn height_stream(&self) -> BoxStream<'static, BlockHeight>;

    async fn get_sealed_block_header(
        &self,
        height: BlockHeight,
    ) -> anyhow::Result<Option<SourcePeer<SealedBlockHeader>>>;

    async fn get_transactions(
        &self,
        block_id: SourcePeer<BlockId>,
    ) -> anyhow::Result<Option<Vec<Transaction>>>;
}

#[cfg_attr(test, automock)]
#[async_trait::async_trait]
pub trait Executor {
    async fn execute_and_commit(
        &self,
        block: SealedBlock,
    ) -> anyhow::Result<ExecutionResult>;
}

pub struct MerkleTree;

#[cfg_attr(test, automock)]
#[async_trait::async_trait]
pub trait DatabasePort {
    async fn get_height_tree(&self, height: BlockHeight) -> anyhow::Result<MerkleTree>;
}
