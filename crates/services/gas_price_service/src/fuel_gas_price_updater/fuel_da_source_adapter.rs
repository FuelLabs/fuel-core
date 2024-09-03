use crate::fuel_gas_price_updater::{
    DaCommitDetails,
    DaCommitSource,
    Result as GasPriceUpdaterResult,
};

#[derive(Default, Clone)]
pub struct FuelDaSource;

impl DaCommitSource for FuelDaSource {
    fn get_da_commit_details(
        &mut self,
    ) -> GasPriceUpdaterResult<Option<DaCommitDetails>> {
        todo!()
    }
}
