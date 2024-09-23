use crate::{
    database::OnChainIterableKeyValueView,
    service::adapters::ConsensusParametersProvider,
};
use fuel_core_gas_price_service::{
    fuel_gas_price_updater::{
        fuel_core_storage_adapter::{
            storage::GasPriceMetadata,
            GasPriceSettings,
            GasPriceSettingsProvider,
        },
        Error as GasPriceError,
        Result as GasPriceResult,
        UpdaterMetadata,
    },
    ports::{
        GasPriceData,
        L2Data,
        MetadataStorage,
    },
};
use fuel_core_storage::{
    transactional::{
        HistoricalView,
        WriteTransaction,
    },
    Result as StorageResult,
    StorageAsMut,
    StorageAsRef,
};
use fuel_core_types::{
    blockchain::{
        block::Block,
        header::ConsensusParametersVersion,
    },
    fuel_tx::Transaction,
    fuel_types::BlockHeight,
};

use crate::database::{
    database_description::gas_price::GasPriceDatabase,
    Database,
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

impl MetadataStorage for Database<GasPriceDatabase> {
    fn get_metadata(
        &self,
        block_height: &BlockHeight,
    ) -> GasPriceResult<Option<UpdaterMetadata>> {
        let metadata = self
            .storage::<GasPriceMetadata>()
            .get(block_height)
            .map_err(|err| GasPriceError::CouldNotFetchMetadata {
                source_error: err.into(),
            })?;
        Ok(metadata.map(|inner| inner.into_owned()))
    }

    fn set_metadata(&mut self, metadata: &UpdaterMetadata) -> GasPriceResult<()> {
        let block_height = metadata.l2_block_height();
        let mut tx = self.write_transaction();
        tx.storage_as_mut::<GasPriceMetadata>()
            .insert(&block_height, metadata)
            .map_err(|err| GasPriceError::CouldNotSetMetadata {
                block_height,
                source_error: err.into(),
            })?;
        tx.commit().map_err(|err| GasPriceError::CouldNotSetMetadata {
            block_height,
            source_error: err.into(),
        })?;
        Ok(())
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
