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
        L2Data,
    },
    v1::metadata::V1AlgorithmConfig,
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

impl From<Config> for V1AlgorithmConfig {
    fn from(value: Config) -> Self {
        V1AlgorithmConfig {
            new_exec_gas_price: value.starting_exec_gas_price,
            min_exec_gas_price: value.min_exec_gas_price,
            exec_gas_price_change_percent: value.exec_gas_price_change_percent,
            l2_block_fullness_threshold_percent: value.exec_gas_price_threshold_percent,
            min_da_gas_price: value.min_da_gas_price,
            max_da_gas_price: value.max_da_gas_price,
            max_da_gas_price_change_percent: value.max_da_gas_price_change_percent,
            da_p_component: value.da_gas_price_p_component,
            da_d_component: value.da_gas_price_d_component,
            normal_range_size: value.activity_normal_range_size,
            capped_range_size: value.activity_capped_range_size,
            decrease_range_size: value.activity_decrease_range_size,
            block_activity_threshold: value.block_activity_threshold,
            da_poll_interval: value.da_poll_interval,
            gas_price_factor: value.da_gas_price_factor,
        }
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
