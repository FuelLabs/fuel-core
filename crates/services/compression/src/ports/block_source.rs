use fuel_core_services::stream::BoxStream;

/// Block with metadata needed for compression
#[cfg_attr(feature = "test-helpers", derive(Default))]
#[derive(Clone, Debug)]
pub struct BlockWithMetadata {
    block: fuel_core_types::blockchain::block::Block,
    // todo: perhaps slice with lifetime? (https://github.com/FuelLabs/fuel-core/issues/2870)
    events: Vec<fuel_core_types::services::executor::Event>,
}

impl BlockWithMetadata {
    /// Create a new block with metadata
    pub fn new(
        block: fuel_core_types::blockchain::block::Block,
        events: Vec<fuel_core_types::services::executor::Event>,
    ) -> Self {
        Self { block, events }
    }

    pub(crate) fn height(&self) -> u32 {
        (*self.block.header().height()).into()
    }

    pub(crate) fn events(&self) -> &[fuel_core_types::services::executor::Event] {
        &self.events
    }

    pub(crate) fn block(&self) -> &fuel_core_types::blockchain::block::Block {
        &self.block
    }
}

/// Type alias for returned value by .subscribe() method on `BlockSource`
pub type BlockStream = BoxStream<BlockWithMetadata>;

/// Port for L2 blocks source
pub trait BlockSource {
    /// Should provide a stream of blocks with metadata
    fn subscribe(&self) -> BoxStream<BlockWithMetadata>;
}
