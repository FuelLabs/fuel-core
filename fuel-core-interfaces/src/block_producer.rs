use crate::model::{
    BlockHeight,
    DaBlockHeight,
    FuelBlock,
};
use anyhow::Result;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::oneshot;

#[derive(Debug)]
pub enum BlockProducerMpsc {
    Produce {
        // add needed information for block to be produced
        height: BlockHeight,
        response: oneshot::Sender<Result<Box<FuelBlock>>>,
    },
    Stop,
}

#[derive(Clone, Debug)]
pub enum BlockProducerBroadcast {
    NewBlockProduced(Arc<FuelBlock>),
}

#[async_trait::async_trait]
pub trait BlockProducer: Send + Sync {
    async fn produce_block(&self, height: BlockHeight) -> Result<FuelBlock>;
}

#[derive(Error, Debug)]
pub enum Error {
    #[error(
        "0 is an invalid block height for production. It is reserved for genesis data."
    )]
    GenesisBlock,
    #[error("Previous block height {0} doesn't exist")]
    MissingBlock(BlockHeight),
    #[error("Best finalized da_height {best} is behind previous block da_height {previous_block}")]
    InvalidDaFinalizationState {
        best: DaBlockHeight,
        previous_block: DaBlockHeight,
    },
}
