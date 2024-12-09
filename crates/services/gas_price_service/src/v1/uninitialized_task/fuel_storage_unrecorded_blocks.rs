use crate::common::{
    fuel_core_storage_adapter::storage::{
        GasPriceColumn,
        UnrecordedBlocksTable,
    },
    utils::BlockInfo::Block,
};
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
use fuel_core_types::{
    fuel_merkle::storage::StorageMutateInfallible,
    fuel_types::BlockHeight,
};
use fuel_gas_price_algorithm::{
    v1,
    v1::UnrecordedBlocks,
};

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
    Storage: KeyValueInspect<Column = GasPriceColumn> + Modifiable,
    Storage: Send + Sync,
{
    fn insert(&mut self, height: v1::Height, bytes: v1::Bytes) -> Result<(), v1::Error> {
        let mut tx = self.inner.write_transaction();
        let block_height = BlockHeight::from(height);
        tx.storage_as_mut::<UnrecordedBlocksTable>()
            .insert(&block_height, &bytes)
            .and_then(|_| tx.commit())
            .map_err(|err| {
                v1::Error::CouldNotInsertUnrecordedBlock(format!("Error: {:?}", err))
            })?;
        Ok(())
    }

    fn remove(&mut self, height: &v1::Height) -> Result<Option<v1::Bytes>, v1::Error> {
        let mut tx = self.inner.write_transaction();
        let block_height = BlockHeight::from(*height);
        let bytes = tx
            .storage_as_mut::<UnrecordedBlocksTable>()
            .take(&block_height)
            .map_err(|err| {
                v1::Error::CouldNotRemoveUnrecordedBlock(format!("Error: {:?}", err))
            })?;
        tx.commit().map_err(|err| {
            v1::Error::CouldNotRemoveUnrecordedBlock(format!("Error: {:?}", err))
        })?;
        Ok(bytes)
    }
}

#[allow(non_snake_case)]
#[cfg(test)]
mod tests {
    use super::*;
    use fuel_core_storage::{
        structured_storage::test::InMemoryStorage,
        transactional::{
            IntoTransaction,
            StorageTransaction,
        },
    };

    fn database() -> StorageTransaction<InMemoryStorage<GasPriceColumn>> {
        InMemoryStorage::default().into_transaction()
    }

    #[test]
    fn insert__remove__round_trip() {
        // given
        let mut storage = FuelStorageUnrecordedBlocks::new(database());
        let height = 8;
        let bytes = 100;

        // when
        storage.insert(height, bytes).unwrap();
        let actual = storage.remove(&height).unwrap();

        // then
        let expected = Some(bytes);
        assert_eq!(expected, actual);
    }

    #[test]
    fn remove__if_not_inserted_returns_none() {
        // given
        let mut storage = FuelStorageUnrecordedBlocks::new(database());
        let height = 8;

        // when
        let maybe_value = storage.remove(&height).unwrap();

        // then
        assert!(maybe_value.is_none());
    }
}
