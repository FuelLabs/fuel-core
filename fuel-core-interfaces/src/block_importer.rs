use crate::model::{BlockHeight, SealedFuelBlock};
use fuel_tx::Bytes32;
use std::sync::Arc;

/// New block included/created event.
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
