use fuel_core_storage::kv_store::{
    KeyValueInspect,
    KeyValueMutate,
};
use fuel_gas_price_algorithm::v1::UnrecordedBlocks;
use storage::UnrecordedBlocksColumn;

pub mod storage;

#[derive(Debug, Clone)]
pub struct FuelStorageUnrecordedBlocks<Storage> {
    inner: Storage,
}

impl<Storage> FuelStorageUnrecordedBlocks<Storage> {
    pub fn new(inner: Storage) -> Self {
        Self { inner }
    }
}

impl<Storage> UnrecordedBlocks for FuelStorageUnrecordedBlocks<Storage>
where
    Storage: KeyValueMutate<Column = UnrecordedBlocksColumn>,
    Storage: Send + Sync,
{
    fn insert(
        &mut self,
        height: fuel_gas_price_algorithm::v1::Height,
        bytes: fuel_gas_price_algorithm::v1::Bytes,
    ) {
        todo!()
    }

    fn remove(
        &mut self,
        height: &fuel_gas_price_algorithm::v1::Height,
    ) -> Option<fuel_gas_price_algorithm::v1::Bytes> {
        todo!()
    }
}
