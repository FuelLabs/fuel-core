use crate::service::adapters::ConsensusParametersProvider;
use fuel_core_gas_price_service::fuel_gas_price_updater::{
    fuel_core_storage_adapter::{
        GasPriceSettings,
        GasPriceSettingsProvider,
    },
    Error as GasPriceError,
    Result as GasPriceResult,
};
use fuel_core_types::blockchain::header::ConsensusParametersVersion;

#[cfg(test)]
mod tests;

impl GasPriceSettingsProvider for ConsensusParametersProvider {
    fn settings(
        &self,
        param_version: &ConsensusParametersVersion,
    ) -> GasPriceResult<GasPriceSettings> {
        self.shared_state
            .get_consensus_parameters(param_version)
            .map(|params| GasPriceSettings {
                gas_price_factor: params.fee_params().gas_price_factor(),
                block_gas_limit: params.block_gas_limit(),
            })
            .map_err(|err| GasPriceError::CouldNotFetchMetadata {
                source_error: err.into(),
            })
    }
}
