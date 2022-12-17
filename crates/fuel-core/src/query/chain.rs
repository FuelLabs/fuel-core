use fuel_core_storage::Result as StorageResult;
use fuel_core_types::{
    blockchain::header::BlockHeader,
    fuel_types::Bytes32
};


#[cfg_attr(test, mockall::automock)]
/// Trait that specifies all the data required by the output message query.
pub trait ChainQueryData {
    fn name(&self) -> StorageResult<String>;
    fn latest_block(&self) -> StorageResult<(BlockHeader, Vec<Bytes32>)>;
}
