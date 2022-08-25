use crate::model::{
    BlockHeight,
    FuelBlock,
};
use tokio::sync::oneshot;

pub enum BlockProducerMpsc {
    Produce {
        // add needed information for block to be produced
        height: BlockHeight,
        response: oneshot::Sender<Box<FuelBlock>>,
    },
    Stop,
}
