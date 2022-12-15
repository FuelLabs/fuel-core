use crate::common::fuel_types::Bytes32;
use fuel_core_types::blockchain::{
    block::Block,
    consensus::ConsensusVote,
    SealedBlock,
};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub enum ImportBlockBroadcast {
    PendingFuelBlockImported {
        block: Arc<Block>,
    },
    /// for blocks that imported in initial sync and in active sync.
    SealedFuelBlockImported {
        block: Arc<SealedBlock>,
        is_created_by_self: bool,
    },
}

impl ImportBlockBroadcast {
    pub fn block(&self) -> &Block {
        match self {
            Self::PendingFuelBlockImported { block } => block.as_ref(),
            Self::SealedFuelBlockImported { block, .. } => &block.as_ref().entity,
        }
    }
}

pub enum ImportBlockMpsc {
    ImportSealedFuelBlock {
        block: Arc<SealedBlock>,
    },
    ImportFuelBlock {
        block: Arc<Block>,
    },
    SealFuelBlock {
        votes: Vec<ConsensusVote>,
        block_id: Bytes32,
    },
    Stop,
}
