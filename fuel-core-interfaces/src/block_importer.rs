use super::model::{BlockHeight, ConsensusVote, FuelBlock, SealedFuelBlock};
use fuel_types::Bytes32;
use std::sync::Arc;

/// Currently just placeholder for new block included and new block created events.
/// TODO remove this after relayer pull request passes
#[derive(Clone, Debug)]
pub enum NewBlockEvent {
    Created(Arc<SealedFuelBlock>),
    Included(Arc<SealedFuelBlock>),
}

impl NewBlockEvent {
    pub fn block(&self) -> &Arc<SealedFuelBlock> {
        match self {
            Self::Created(block) => block,
            Self::Included(block) => block,
        }
    }
    pub fn height(&self) -> BlockHeight {
        self.block().header.height
    }

    pub fn id(&self) -> Bytes32 {
        self.block().header.id()
    }
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
