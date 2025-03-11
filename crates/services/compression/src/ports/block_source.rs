/// Type alias for fuel_core_types::blockchain::block::Block
pub type Block = fuel_core_types::blockchain::block::Block;

/// Port for L2 blocks source
/// Each time `.next()` is called, the service expects the same behaviour as an iterator.
pub trait BlockSource {
    /// Get the next block from the source
    fn next(
        &self,
    ) -> impl core::future::Future<Output = crate::Result<Option<Block>>> + Send + Sync;
}
