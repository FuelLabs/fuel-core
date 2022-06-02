use super::model::{ConsensusVote, FuelBlock, SealedFuelBlock};
use fuel_types::Bytes32;
use std::sync::Arc;

/// Currently just placeholder for new block included and new block created events.
/// TODO remove this after relayer pull request passes
#[derive(Clone, Debug)]
pub enum NewBlockEvent {
    /// send this to eth
    NewBlockCreated { height: u64 },
    NewBlockIncluded {
        height: u64,
        /// height where we are finalizing stake and token deposits.
        da_height: u64,
    },
}

#[derive(Clone, Debug)]
pub enum ImportBlockBroadcast {
    PendingBlockImported {
        block: Arc<FuelBlock>,
    },
    /// for blocks that imported in initial sync and in active sync.
    SealedFuelBlockImported {
        block: Arc<SealedFuelBlock>,
        is_created_by_self: bool,
    },
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
