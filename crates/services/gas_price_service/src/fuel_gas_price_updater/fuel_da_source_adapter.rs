use crate::fuel_gas_price_updater::{
    DaCommitDetails,
    DaCommitSource,
    Result as GasPriceUpdaterResult,
};

#[derive(Default, Clone)]
pub struct FuelDaSource;

#[async_trait::async_trait]
impl DaCommitSource for FuelDaSource {
    async fn get_da_commit_details(&mut self) -> GasPriceUpdaterResult<DaCommitDetails> {
        todo!()
    }
}
