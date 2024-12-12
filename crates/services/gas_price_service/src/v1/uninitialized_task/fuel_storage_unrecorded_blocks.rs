use crate::common::fuel_core_storage_adapter::storage::{
    GasPriceColumn,
    UnrecordedBlocksTable,
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
    Error as StorageError,
    StorageAsMut,
    StorageAsRef,
    StorageMutate,
};
use fuel_core_types::fuel_merkle::storage::StorageMutateInfallible;
use fuel_gas_price_algorithm::{
    v1,
    v1::UnrecordedBlocks,
};

pub trait AsUnrecordedBlocks {
    type Wrapper<'a>: UnrecordedBlocks
    where
        Self: 'a;

    fn as_unrecorded_blocks(&mut self) -> Self::Wrapper<'_>;
}

impl<S> AsUnrecordedBlocks for S
where
    S: StorageMutate<UnrecordedBlocksTable, Error = StorageError>,
{
    type Wrapper<'a> = FuelStorageUnrecordedBlocks<&'a mut Self>
        where
            Self: 'a;

    fn as_unrecorded_blocks(&mut self) -> Self::Wrapper<'_> {
        FuelStorageUnrecordedBlocks::new(self)
    }
}

#[derive(Debug, Clone)]
pub struct FuelStorageUnrecordedBlocks<Storage> {
    inner: Storage,
}

impl<Storage> FuelStorageUnrecordedBlocks<Storage> {
    pub fn new(inner: Storage) -> Self {
        Self { inner }
    }
}

impl<S> UnrecordedBlocks for FuelStorageUnrecordedBlocks<S>
where
    S: StorageMutate<UnrecordedBlocksTable, Error = StorageError>,
{
    fn insert(&mut self, height: v1::Height, bytes: v1::Bytes) -> Result<(), String> {
        self.inner
            .storage_as_mut::<UnrecordedBlocksTable>()
            .insert(&height.into(), &bytes)
            .map_err(|err| format!("Error: {:?}", err))?;
        Ok(())
    }

    fn remove(&mut self, height: &v1::Height) -> Result<Option<v1::Bytes>, String> {
        let bytes = self
            .inner
            .storage_as_mut::<UnrecordedBlocksTable>()
            .take(&(*height).into())
            .map_err(|err| format!("Error: {:?}", err))?;
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
