use super::model::{
    FuelBlock,
    SealedFuelBlock,
};
use anyhow::Result;
use std::sync::Arc;
use tokio::sync::oneshot;

pub enum BftMpsc {
    CheckBlockConsensus {
        block: Arc<SealedFuelBlock>,
        ret: oneshot::Sender<Result<()>>,
    },
    CheckBlockLeader {
        block: Arc<FuelBlock>,
        ret: oneshot::Sender<Result<()>>,
    },
    Stop,
    Start,
}
