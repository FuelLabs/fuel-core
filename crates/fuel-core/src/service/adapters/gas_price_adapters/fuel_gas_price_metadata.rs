use fuel_core_gas_price_service::fuel_gas_price_updater::{
    MetadataStorage,
    Result as GasPriceResult,
    UpdaterMetadata,
};

pub struct FuelGasPriceMetadata<Database> {
    _database: Database,
}

#[async_trait::async_trait]
impl<Database: Send + Sync> MetadataStorage for FuelGasPriceMetadata<Database> {
    async fn get_metadata(&self) -> GasPriceResult<Option<UpdaterMetadata>> {
        todo!()
    }

    async fn set_metadata(&mut self, _metadata: UpdaterMetadata) -> GasPriceResult<()> {
        todo!()
    }
}
