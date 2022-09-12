use super::model::{
    ConsensusVote,
    FuelBlock,
    SealedFuelBlock,
};
use crate::common::fuel_types::Bytes32;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub enum ImportBlockBroadcast {
    PendingFuelBlockImported {
        block: Arc<FuelBlock>,
    },
    /// for blocks that imported in initial sync and in active sync.
    SealedFuelBlockImported {
        block: Arc<SealedFuelBlock>,
        is_created_by_self: bool,
    },
}

impl ImportBlockBroadcast {
    pub fn block(&self) -> &FuelBlock {
        match self {
            Self::PendingFuelBlockImported { block } => block.as_ref(),
            Self::SealedFuelBlockImported { block, .. } => block.as_ref(),
        }
    }
}

pub enum ImportBlockMpsc {
    ImportSealedFuelBlock {
        block: Arc<SealedFuelBlock>,
    },
    ImportFuelBlock {
        block: Arc<FuelBlock>,
    },
    SealFuelBlock {
        votes: Vec<ConsensusVote>,
        block_id: Bytes32,
    },
    Stop,
}
