use fuel_core_gas_price_service::fuel_gas_price_updater::{
    MetadataStorage,
    Result as GasPriceResult,
    UpdaterMetadata,
};
use fuel_core_storage::{
    tables::GasPriceMetadata,
    transactional::AtomicView,
    StorageAsRef,
    StorageInspect,
};
use fuel_core_types::fuel_types::BlockHeight;

#[cfg(test)]
mod tests;

pub struct FuelGasPriceMetadataStorage<Database> {
    _database: Database,
}

#[async_trait::async_trait]
impl<Database> MetadataStorage for FuelGasPriceMetadataStorage<Database>
where
    Database: AtomicView<Height = BlockHeight>,
    Database::View: StorageAsRef,
    Database::View: StorageInspect<GasPriceMetadata>,
    // <Database::View as StorageInspect<GasPriceMetadata>>::Error: Into<anyhow::Error>,
{
    async fn get_metadata(&self) -> GasPriceResult<Option<UpdaterMetadata>> {
        todo!()
    }

    async fn set_metadata(&mut self, _metadata: UpdaterMetadata) -> GasPriceResult<()> {
        todo!()
    }
}
