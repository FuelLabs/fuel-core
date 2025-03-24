use fuel_core_services::stream::BoxStream;
pub(crate) use fuel_core_types::services::block_importer::SharedImportResult as BlockWithMetadata;

pub(crate) type BlockHeight = u32;

pub(crate) trait BlockWithMetadataExt {
    fn height(&self) -> &BlockHeight;
    fn block(&self) -> &fuel_core_types::blockchain::block::Block;
    fn events(&self) -> &[fuel_core_types::services::executor::Event];
    #[cfg(test)]
    fn default() -> BlockWithMetadata;
}

impl BlockWithMetadataExt for BlockWithMetadata {
    fn height(&self) -> &BlockHeight {
        <Self as BlockWithMetadataExt>::block(self)
            .header()
            .height()
    }

    fn block(&self) -> &fuel_core_types::blockchain::block::Block {
        &self.sealed_block.entity
    }

    fn events(&self) -> &[fuel_core_types::services::executor::Event] {
        self.events.as_ref()
    }

    #[cfg(test)]
    fn default() -> BlockWithMetadata {
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
