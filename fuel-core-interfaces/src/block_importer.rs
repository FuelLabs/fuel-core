use std::sync::Arc;

use fuel_tx::Bytes32;

use crate::model::{BlockHeight, SealedFuelBlock};

/// Currently just placeholder for new block included and new block created events.
#[derive(Clone, Debug)]
pub enum NewBlockEvent {
    /// send this to eth
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
