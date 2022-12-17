use fuel_core_storage::Result as StorageResult;
use fuel_core_types::{
    blockchain::header::BlockHeader,
    fuel_types::Bytes32,
};

#[cfg_attr(test, mockall::automock)]
/// Trait that specifies all the data required by the output message query.
pub trait BlockQueryData {
    fn block(
        &self,
        id: Bytes32,
    ) -> std::result::Result<StorageResult<(BlockHeader, Vec<Bytes32>)>, anyhow::Error>;
    fn block_id(&self, height: u64) -> std::result::Result<Bytes32, anyhow::Error>;
}
