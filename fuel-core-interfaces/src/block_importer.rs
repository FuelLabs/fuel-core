use std::sync::Arc;

use crate::primitive_types::SealedFuelBlock;

/// Currently just placeholder for new block included and new block created events.
#[derive(Clone, Debug)]
pub enum NewBlockEvent {
    /// send this to eth
    NewBlockCreated(Arc<SealedFuelBlock>),
    NewBlockIncluded(Arc<SealedFuelBlock>),
}
