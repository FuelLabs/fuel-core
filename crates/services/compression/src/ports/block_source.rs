use fuel_core_services::stream::BoxStream;
pub(crate) use fuel_core_types::services::block_importer::SharedImportResult as BlockWithMetadata;

pub(crate) type BlockHeight = u32;

// we can't define a newtype that wraps over SharedImportResult because its `dyn Deref`
pub(crate) mod block_helpers {
    use super::*;

    /// Get events from BlockWithMetadata
    pub(crate) fn events(
        block_with_metadata: &BlockWithMetadata,
    ) -> &[fuel_core_types::services::executor::Event] {
        block_with_metadata.events.as_ref()
    }

    /// Get sealed block from BlockWithMetadata
    pub(crate) fn block(
        block_with_metadata: &BlockWithMetadata,
    ) -> &fuel_core_types::blockchain::block::Block {
        &block_with_metadata.sealed_block.entity
    }

    /// Get block height from BlockWithMetadata
    pub(crate) fn height(block_with_metadata: &BlockWithMetadata) -> &BlockHeight {
        block(block_with_metadata).header().height()
    }

    /// Get the default BlockWithMetadata
    #[cfg(test)]
    pub fn default() -> BlockWithMetadata {
        std::sync::Arc::new(
            fuel_core_types::services::block_importer::ImportResult::default(),
        )
    }
}

/// Type alias for returned value by .subscribe() method on `BlockSource`
pub type BlockStream = BoxStream<BlockWithMetadata>;

/// Port for L2 blocks source
pub trait BlockSource {
    /// Should provide a stream of blocks with metadata
    fn subscribe(&self) -> BlockStream;
}
