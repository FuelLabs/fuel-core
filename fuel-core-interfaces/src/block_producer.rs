use crate::{
    common::fuel_tx::{
        Receipt,
        Transaction,
    },
    executor::ExecutionResult,
    model::{
        BlockHeight,
        DaBlockHeight,
        FuelBlock,
    },
};
use anyhow::Result;
use fuel_vm::prelude::Word;
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
    // TODO: Right now production and execution of the block is one step, but in the future,
    //  `produce_block` should only produce a block without affecting the blockchain state.
    async fn produce_and_execute_block(
        &self,
        height: BlockHeight,
        max_gas: Word,
    ) -> Result<ExecutionResult>;

    async fn dry_run(
        &self,
        transaction: Transaction,
        height: Option<BlockHeight>,
        utxo_validation: Option<bool>,
    ) -> Result<Vec<Receipt>>;
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

#[async_trait::async_trait]
pub trait Relayer: Sync + Send {
    /// Get the best finalized height from the DA layer
    async fn get_best_finalized_da_height(&self) -> Result<DaBlockHeight>;
}
