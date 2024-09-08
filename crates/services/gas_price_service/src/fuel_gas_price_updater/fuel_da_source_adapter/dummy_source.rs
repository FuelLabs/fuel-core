use crate::fuel_gas_price_updater::{
    service::Result as DaGasPriceSourceResult,
    DaGasPrice,
    DaGasPriceSource,
};

pub struct DummyDaGasPriceSource;

#[async_trait::async_trait]
impl DaGasPriceSource for DummyDaGasPriceSource {
    async fn get(&mut self) -> DaGasPriceSourceResult<DaGasPrice> {
        Ok(DaGasPrice::default())
    }
}
