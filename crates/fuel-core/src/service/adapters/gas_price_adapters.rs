use crate::{
    database::OnChainIterableKeyValueView,
    service::adapters::ConsensusParametersProvider,
};
use fuel_core_gas_price_service::{
    common::{
        fuel_core_storage_adapter::{
            GasPriceSettings,
            GasPriceSettingsProvider,
        },
        utils::{
            Error as GasPriceError,
            Result as GasPriceResult,
        },
    },
    ports::{
        GasPriceData,
        GasPriceServiceConfig,
        L2Data,
    },
};
use fuel_core_storage::{
    transactional::HistoricalView,
    Result as StorageResult,
};
use fuel_core_types::{
    blockchain::{
        block::Block,
        header::ConsensusParametersVersion,
    },
    fuel_tx::Transaction,
    fuel_types::BlockHeight,
};

use crate::{
    database::{
        database_description::gas_price::GasPriceDatabase,
        Database,
    },
    service::Config,
};

#[cfg(test)]
mod tests;

impl L2Data for OnChainIterableKeyValueView {
    fn latest_height(&self) -> StorageResult<BlockHeight> {
        self.latest_height()
    }

    fn get_block(
        &self,
        height: &BlockHeight,
    ) -> StorageResult<Option<Block<Transaction>>> {
        self.get_full_block(height)
    }
}

impl GasPriceData for Database<GasPriceDatabase> {
    fn latest_height(&self) -> Option<BlockHeight> {
        HistoricalView::latest_height(self)
    }
}

impl From<Config> for GasPriceServiceConfig {
    fn from(value: Config) -> Self {
        GasPriceServiceConfig::new(
            value.min_gas_price,
            value.starting_gas_price,
            value.gas_price_change_percent,
            value.gas_price_threshold_percent,
        )
    }
}

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
