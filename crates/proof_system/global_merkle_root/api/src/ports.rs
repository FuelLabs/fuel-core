use fuel_core_storage::Error as StorageError;
use fuel_core_types::{
    fuel_tx::Bytes32,
    fuel_types::BlockHeight,
};

/// Represents the ability to retrieve a state root at a given block height
pub trait GetStateRoot {
    /// Get the state root at the given height
    fn state_root_at(&self, height: BlockHeight)
        -> Result<Option<Bytes32>, StorageError>;
}
