use std::sync::Arc;

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

pub struct Ports<P, E, C>
where
    P: PeerToPeerPort + 'static,
    E: ExecutorPort + 'static,
    C: ConsensusPort + 'static,
{
    pub p2p: Arc<P>,
    pub executor: Arc<E>,
    pub consensus: Arc<C>,
}

#[cfg_attr(test, automock)]
#[async_trait::async_trait]
pub trait PeerToPeerPort {
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
pub trait ConsensusPort {
    async fn check_sealed_header(
        &self,
        header: &SealedBlockHeader,
    ) -> anyhow::Result<bool>;
}

#[cfg_attr(test, automock)]
#[async_trait::async_trait]
pub trait ExecutorPort {
    async fn execute_and_commit(
        &self,
        block: SealedBlock,
    ) -> anyhow::Result<ExecutionResult>;
}
