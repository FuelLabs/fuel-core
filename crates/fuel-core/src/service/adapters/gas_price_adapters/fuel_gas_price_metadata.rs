use crate::service::adapters::gas_price_adapters::fuel_gas_price_metadata::storage::GasPriceMetadata;
use fuel_core_gas_price_service::fuel_gas_price_updater::{
    Error as GasPriceError,
    MetadataStorage,
    Result as GasPriceResult,
    UpdaterMetadata,
};
use fuel_core_storage::{
    transactional::{
        AtomicView,
        Modifiable,
        StorageTransaction,
        WriteTransaction,
    },
    StorageAsMut,
    StorageAsRef,
    StorageInspect,
    StorageMutate,
};
use fuel_core_types::fuel_types::BlockHeight;

#[cfg(test)]
mod tests;

pub mod storage;

pub struct FuelGasPriceMetadataStorage<Database> {
    database: Database,
}

#[async_trait::async_trait]
impl<Database> MetadataStorage for FuelGasPriceMetadataStorage<Database>
where
    Database: AtomicView,
    Database::LatestView: StorageAsRef,
    Database::LatestView: StorageInspect<GasPriceMetadata>,
    <Database::LatestView as StorageInspect<GasPriceMetadata>>::Error:
        Into<anyhow::Error>,
    Database: Modifiable,
    Database: WriteTransaction,
    for<'a> StorageTransaction<&'a mut Database>: StorageMutate<GasPriceMetadata>,
    for<'a> <StorageTransaction<&'a mut Database> as StorageInspect<GasPriceMetadata>>::Error:
        Into<anyhow::Error>,
{
    async fn get_metadata(
        &self,
        block_height: &BlockHeight,
    ) -> GasPriceResult<Option<UpdaterMetadata>> {
        let view = self.database.latest_view().map_err(|err| {
            GasPriceError::CouldNotFetchMetadata {
                block_height: *block_height,
                source_error: err.into(),
            }
        })?;
        let metadata = view
            .storage::<GasPriceMetadata>()
            .get(block_height)
            .map_err(|err| GasPriceError::CouldNotFetchMetadata {
                block_height: *block_height,
                source_error: err.into(),
            })?;
        Ok(metadata.map(|inner| inner.into_owned()))
    }

    async fn set_metadata(&mut self, metadata: UpdaterMetadata) -> GasPriceResult<()> {
        let block_height = metadata.l2_block_height();
        let mut tx = self.database.write_transaction();
        tx.storage_as_mut::<GasPriceMetadata>()
            .insert(&block_height, &metadata)
            .map_err(|err| GasPriceError::CouldNotSetMetadata {
                block_height,
                source_error: err.into(),
            })?;
        tx.commit()
            .map_err(|err| GasPriceError::CouldNotSetMetadata {
                block_height,
                source_error: err.into(),
            })?;
        Ok(())
    }
}
