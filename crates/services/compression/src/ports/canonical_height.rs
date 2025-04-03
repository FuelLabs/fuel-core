use super::block_source::BlockHeight;

/// Canonical height port
/// Should provide the canonical height of the blockchain.
pub trait CanonicalHeight: Send + Sync {
    /// Returns the canonical height of the blockchain.
    fn get(&self) -> Option<BlockHeight>;
}
