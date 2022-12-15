use anyhow::Result;
use fuel_core_types::blockchain::{
    block::Block,
    SealedBlock,
};
use std::sync::Arc;
use tokio::sync::oneshot;

pub enum BftMpsc {
    CheckBlockConsensus {
        block: Arc<SealedBlock>,
        ret: oneshot::Sender<Result<()>>,
    },
    CheckBlockLeader {
        block: Arc<Block>,
        ret: oneshot::Sender<Result<()>>,
    },
    Stop,
    Start,
}
