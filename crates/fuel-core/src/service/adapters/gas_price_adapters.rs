use crate::{
    database::OnChainIterableKeyValueView,
    service::adapters::ConsensusParametersProvider,
};
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
    not_found,
    tables::{
        FuelBlocks,
        Transactions,
    },
    Result as StorageResult,
    StorageAsRef,
};
use fuel_core_types::{
    blockchain::{
        block::{
            Block,
        },
        header::ConsensusParametersVersion,
    },
    fuel_tx::{
        Transaction,
    },
    fuel_types::BlockHeight,
};

#[cfg(test)]
mod tests;

impl L2Data for OnChainIterableKeyValueView {
    fn latest_height(&self) -> StorageResult<BlockHeight> {
        self.latest_height()
    }

    fn get_block(&self, height: &BlockHeight) -> StorageResult<Block<Transaction>> {
        let block = self
            .storage::<FuelBlocks>()
            .get(height)
            .map(|block| block.map(|block| block.as_ref().clone()))?
            .ok_or(not_found!("FuelBlock"))?;

        let mut transactions = vec![];
        for tx_id in block.transactions() {
            let tx = self
                .storage::<Transactions>()
                .get(tx_id)
                .map(|tx| tx.map(|tx| tx.as_ref().clone()))?
                .ok_or(not_found!("Transaction"))?;
            transactions.push(tx);
        }
        Ok(block.uncompress(transactions))
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
