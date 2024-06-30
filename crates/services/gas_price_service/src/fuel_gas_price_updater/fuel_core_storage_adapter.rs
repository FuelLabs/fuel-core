use crate::fuel_gas_price_updater::{
    fuel_core_storage_adapter::database::{
        GasPriceColumn,
        GasPriceMetadata,
    },
    Error,
    MetadataStorage,
    Result,
    UpdaterMetadata,
};
use fuel_core_storage::{
    kv_store::KeyValueInspect,
    structured_storage::StructuredStorage,
    transactional::{
        Modifiable,
        WriteTransaction,
    },
    StorageAsMut,
    StorageAsRef,
};
use fuel_core_types::fuel_types::BlockHeight;

#[cfg(test)]
mod metadata_tests;

pub mod database;

#[async_trait::async_trait]
impl<Database> MetadataStorage for StructuredStorage<Database>
where
    Database: KeyValueInspect<Column = GasPriceColumn> + Modifiable,
    Database: Send + Sync,
{
    async fn get_metadata(
        &self,
        block_height: &BlockHeight,
    ) -> Result<Option<UpdaterMetadata>> {
        let metadata = self
            .storage::<GasPriceMetadata>()
            .get(block_height)
            .map_err(|err| Error::CouldNotFetchMetadata {
                block_height: *block_height,
                source_error: err.into(),
            })?;
        Ok(metadata.map(|inner| inner.into_owned()))
    }

    async fn set_metadata(&mut self, metadata: UpdaterMetadata) -> Result<()> {
        let block_height = metadata.l2_block_height();
        let mut tx = self.write_transaction();
        tx.storage_as_mut::<GasPriceMetadata>()
            .insert(&block_height, &metadata)
            .map_err(|err| Error::CouldNotSetMetadata {
                block_height,
                source_error: err.into(),
            })?;
        tx.commit().map_err(|err| Error::CouldNotSetMetadata {
            block_height,
            source_error: err.into(),
        })?;
        Ok(())
    }
}
