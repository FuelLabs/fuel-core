use fuel_core_services::stream::BoxStream;
use fuel_core_types::blockchain::block::Block;
pub(crate) use fuel_core_types::services::block_importer::SharedImportResult as BlockWithMetadata;

pub(crate) type BlockHeight = u32;

pub(crate) trait BlockWithMetadataExt {
    fn height(&self) -> &BlockHeight;
    fn block(&self) -> &fuel_core_types::blockchain::block::Block;
    #[cfg(test)]
    fn default() -> Self;
    #[cfg(test)]
    fn test_block_with_height(height: BlockHeight) -> Self;
}

impl BlockWithMetadataExt for BlockWithMetadata {
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

    #[cfg(test)]
    fn test_block_with_height(height: BlockHeight) -> Self {
        use fuel_core_types::services::block_importer::ImportResult;

        let mut import_result = ImportResult::default();
        import_result
            .sealed_block
            .entity
            .header_mut()
            .set_block_height(height.into());
        std::sync::Arc::new(import_result.wrap())
    }
}

/// Type alias for returned value by .subscribe() method on `BlockSource`
pub type BlockStream = BoxStream<BlockWithMetadata>;

/// Represents either the Genesis Block or a block at a specific height
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum BlockAt {
    /// Block at a specific height
    Specific(BlockHeight),
    /// Genesis block
    Genesis,
}

/// Port for L2 blocks source
pub trait BlockSource: Send + Sync {
    /// Should provide a stream of blocks with metadata
    fn subscribe(&self) -> BlockStream;
    /// Should provide the block at a given height
    fn get_block(&self, height: BlockAt) -> anyhow::Result<Block>;
}
