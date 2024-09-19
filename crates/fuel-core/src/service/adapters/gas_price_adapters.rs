use crate::service::adapters::ConsensusParametersProvider;
use fuel_core_gas_price_service::{
    fuel_gas_price_updater::{
        fuel_core_storage_adapter::{
            GasPriceSettings,
            GasPriceSettingsProvider,
        },
        Error as GasPriceError,
        Result as GasPriceResult,
    },
    ports::L2Data,
};
use fuel_core_storage::{
    tables::{
        FuelBlocks,
        Transactions,
    },
    Result as StorageResult,
    StorageAsRef,
};
use fuel_core_types::{
    blockchain::{
        block::CompressedBlock,
        header::ConsensusParametersVersion,
    },
    fuel_tx::{
        Transaction,
        TxId,
    },
    fuel_types::BlockHeight,
};

use crate::database::OnChainIterableKeyValueView;

#[cfg(test)]
mod tests;

impl L2Data for OnChainIterableKeyValueView {
    fn latest_height(&self) -> StorageResult<BlockHeight> {
        self.latest_height()
    }

    fn get_block(&self, height: &BlockHeight) -> StorageResult<Option<CompressedBlock>> {
        self.storage::<FuelBlocks>()
            .get(height)
            .map(|block| block.map(|block| block.as_ref().clone()))
    }

    fn get_transaction(&self, tx_id: &TxId) -> StorageResult<Option<Transaction>> {
        self.storage::<Transactions>()
            .get(tx_id)
            .map(|tx| tx.map(|tx| tx.as_ref().clone()))
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
