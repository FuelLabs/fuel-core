use crate::model::{
    BlockHeight,
    FuelBlock,
};
use std::sync::Arc;
use tokio::sync::oneshot;

#[derive(Debug)]
pub enum BlockProducerMpsc {
    Produce {
        // add needed information for block to be produced
        height: BlockHeight,
        response: oneshot::Sender<anyhow::Result<Arc<Box<FuelBlock>>>>,
    },
    Stop,
}

#[derive(Clone, Debug)]
pub enum BlockProducerBroadcast {
    NewBlockProduced(Arc<Box<FuelBlock>>),
}
