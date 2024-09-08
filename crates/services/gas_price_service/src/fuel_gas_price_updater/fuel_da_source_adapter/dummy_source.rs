use crate::fuel_gas_price_updater::{
    service::Result as DaGasPriceSourceResult,
    DaGasPriceSource,
    RawDaGasPrice,
};

pub struct DummyDaGasPriceSource;

#[async_trait::async_trait]
impl DaGasPriceSource for DummyDaGasPriceSource {
    async fn get(&mut self) -> DaGasPriceSourceResult<RawDaGasPrice> {
        Ok(RawDaGasPrice::default())
    }
}
