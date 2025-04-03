use fuel_core_services::stream::BoxStream;
pub(crate) use fuel_core_types::services::block_importer::SharedImportResult as BlockWithMetadata;

pub(crate) type BlockHeight = u32;

pub(crate) trait BlockWithMetadataExt {
    fn height(&self) -> &BlockHeight;
    fn events(&self) -> &[fuel_core_types::services::executor::Event];
    fn block(&self) -> &fuel_core_types::blockchain::block::Block;
    #[cfg(test)]
    fn default() -> Self;
}

impl BlockWithMetadataExt for BlockWithMetadata {
    fn events(&self) -> &[fuel_core_types::services::executor::Event] {
        self.events.as_ref()
    }

    fn height(&self) -> &BlockHeight {
        self.block().header().height()
    }

    fn block(&self) -> &fuel_core_types::blockchain::block::Block {
        &self.sealed_block.entity
    }

    #[cfg(test)]
    fn default() -> Self {
        use fuel_core_types::services::block_importer::ImportResult;

        std::sync::Arc::new(ImportResult::default().wrap())
    }
}

/// Type alias for returned value by .subscribe() method on `BlockSource`
pub type BlockStream = BoxStream<BlockWithMetadata>;

/// Port for L2 blocks source
pub trait BlockSource {
    /// Should provide a stream of blocks with metadata
    fn subscribe(&self) -> BlockStream;
}
