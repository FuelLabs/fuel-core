use std::future::Future;

use fuel_core_global_merkle_root_storage::column::TableColumn;
use fuel_core_storage::{
    kv_store::KeyValueInspect,
    merkle::column::MerkleizedColumn,
    transactional::Modifiable,
};
use fuel_core_types::blockchain::block::Block;

/// A stream of blocks
pub trait BlockStream {
    /// Error type
    type Error;

    /// Get the next block
    fn next(&mut self) -> impl Future<Output = Result<Block, Self::Error>> + Send;
}

/// The storage requirements for the merkle root service
pub trait ServiceStorage:
    KeyValueInspect<Column = MerkleizedColumn<TableColumn>> + Modifiable
{
}

impl<T> ServiceStorage for T where
    T: KeyValueInspect<Column = MerkleizedColumn<TableColumn>> + Modifiable
{
}
