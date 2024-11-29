use crate::v1::uninitialized_task::fuel_storage_unrecorded_blocks::storage::UnrecordedBlocksTable;
use fuel_core_storage::{
    kv_store::{
        KeyValueInspect,
        KeyValueMutate,
    },
    transactional::{
        Modifiable,
        WriteTransaction,
    },
    StorageAsMut,
    StorageAsRef,
};
use fuel_core_types::fuel_merkle::storage::StorageMutateInfallible;
use fuel_gas_price_algorithm::{
    v1,
    v1::UnrecordedBlocks,
};
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
    Storage: KeyValueInspect<Column = UnrecordedBlocksColumn> + Modifiable,
    Storage: Send + Sync,
{
    fn insert(&mut self, height: v1::Height, bytes: v1::Bytes) -> Result<(), v1::Error> {
        let mut tx = self.inner.write_transaction();
        tx.storage_as_mut::<UnrecordedBlocksTable>()
            .insert(&height, &bytes)
            .and_then(|_| tx.commit())
            .map_err(|err| {
                v1::Error::CouldNotInsertUnrecordedBlock(format!("Error: {:?}", err))
            })?;
        Ok(())
    }

    fn remove(&mut self, height: &v1::Height) -> Result<Option<v1::Bytes>, v1::Error> {
        let mut tx = self.inner.write_transaction();
        let bytes = tx
            .storage_as_mut::<UnrecordedBlocksTable>()
            .take(height)
            .map_err(|err| {
                v1::Error::CouldNotRemoveUnrecordedBlock(format!("Error: {:?}", err))
            })?;
        tx.commit().map_err(|err| {
            v1::Error::CouldNotRemoveUnrecordedBlock(format!("Error: {:?}", err))
        })?;
        Ok(bytes)
    }
}
