use crate::{
    database::OnChainIterableKeyValueView,
    service::adapters::ChainStateInfoProvider,
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
    service::{
        config::GasPriceConfig,
        Config,
    },
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
        let GasPriceConfig {
            starting_exec_gas_price,
            exec_gas_price_change_percent,
            min_exec_gas_price,
            exec_gas_price_threshold_percent,
            da_committer_url: _,
            da_poll_interval,
            da_gas_price_factor,
            starting_recorded_height,
            min_da_gas_price,
            max_da_gas_price,
            max_da_gas_price_change_percent,
            da_gas_price_p_component,
            da_gas_price_d_component,
            gas_price_metrics,
            activity_normal_range_size,
            activity_capped_range_size,
            activity_decrease_range_size,
            block_activity_threshold,
        } = value.gas_price_config;
        V1AlgorithmConfig {
            new_exec_gas_price: starting_exec_gas_price,
            min_exec_gas_price,
            exec_gas_price_change_percent: *exec_gas_price_change_percent as u16,
            l2_block_fullness_threshold_percent: *exec_gas_price_threshold_percent,
            min_da_gas_price,
            max_da_gas_price,
            max_da_gas_price_change_percent: *max_da_gas_price_change_percent as u16,
            da_p_component: da_gas_price_p_component,
            da_d_component: da_gas_price_d_component,
            normal_range_size: activity_normal_range_size,
            capped_range_size: activity_capped_range_size,
            decrease_range_size: activity_decrease_range_size,
            block_activity_threshold,
            da_poll_interval,
            gas_price_factor: da_gas_price_factor,
            starting_recorded_height: starting_recorded_height.map(BlockHeight::from),
            record_metrics: gas_price_metrics,
        }
    }
}

impl GasPriceSettingsProvider for ChainStateInfoProvider {
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
