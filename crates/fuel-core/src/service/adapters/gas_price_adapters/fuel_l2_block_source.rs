use anyhow::anyhow;
use fuel_core_gas_price_service::fuel_gas_price_updater::{
    BlockInfo,
    Error as GasPriceError,
    L2BlockSource,
    Result as GasPriceResult,
};
use fuel_core_storage::{
    tables::{
        ConsensusParametersVersions,
        FuelBlocks,
        Transactions,
    },
    transactional::AtomicView,
    StorageAsRef,
    StorageInspect,
};
use fuel_core_types::{
    blockchain::block::Block,
    fuel_tx::Transaction,
    fuel_types::BlockHeight,
};
use std::time::Duration;

#[cfg(test)]
mod tests;

pub struct FuelL2BlockSource<Database> {
    frequency: Duration,
    database: Database,
}

// TODO: use real values for all the fields
fn get_block_info(block: &Block<Transaction>, _block_gas_limit: u64) -> BlockInfo {
    BlockInfo {
        height: (*block.header().height()).into(),
        fullness: (0, 0),
        block_bytes: 0,
        gas_price: 0,
    }
}

impl<Database> FuelL2BlockSource<Database>
where
    Database: AtomicView<Height = BlockHeight>,
    Database::View: StorageAsRef,
    Database::View: StorageInspect<FuelBlocks>,
    Database::View: StorageInspect<Transactions>,
    <Database::View as StorageInspect<FuelBlocks>>::Error: Into<anyhow::Error>,
    <Database::View as StorageInspect<Transactions>>::Error: Into<anyhow::Error>,
{
    // TODO: Use refs instead of owned values?
    fn get_full_block(&self, height: BlockHeight) -> GasPriceResult<Block<Transaction>> {
        let view = self.database.latest_view();
        let block = view
            .storage::<FuelBlocks>()
            .get(&height)
            .map_err(|source_error| GasPriceError::CouldNotFetchL2Block {
                block_height: height,
                source_error: source_error.into(),
            })?
            .ok_or(GasPriceError::CouldNotFetchL2Block {
                block_height: height,
                source_error: anyhow!(
                    "Block not found in storage despite being at height {}.",
                    height
                ),
            })?;
        let mut txs: Vec<Transaction> = Vec::new();
        for tx_id in block.transactions() {
            let transaction = view
                .storage::<Transactions>()
                .get(tx_id)
                .map_err(|source_error| GasPriceError::CouldNotFetchL2Block {
                    block_height: height,
                    source_error: source_error.into(),
                })?
                .ok_or(GasPriceError::CouldNotFetchL2Block {
                    block_height: height,
                    source_error: anyhow!("Transaction not found in storage: {tx_id:?}",),
                })?;
            txs.push(transaction.into_owned());
        }
        let uncompressed_block = block.into_owned().uncompress(txs);
        Ok(uncompressed_block)
    }
}

#[async_trait::async_trait]
impl<Database> L2BlockSource for FuelL2BlockSource<Database>
where
    Database: AtomicView<Height = BlockHeight>,
    Database::View: StorageAsRef,
    Database::View: StorageInspect<FuelBlocks>,
    Database::View: StorageInspect<Transactions>,
    Database::View: StorageInspect<ConsensusParametersVersions>,
    <Database::View as StorageInspect<FuelBlocks>>::Error: Into<anyhow::Error>,
    <Database::View as StorageInspect<Transactions>>::Error: Into<anyhow::Error>,
    <Database::View as StorageInspect<ConsensusParametersVersions>>::Error:
        Into<anyhow::Error>,
{
    async fn get_l2_block(&self, height: BlockHeight) -> GasPriceResult<BlockInfo> {
        // TODO: Add an escape route for loop
        loop {
            let latest_height = self.database.latest_height().unwrap_or(0.into());
            if latest_height < height {
                tokio::time::sleep(self.frequency).await;
            } else {
                let block = self.get_full_block(height)?;
                let view = self.database.latest_view();
                let param_version = block.header().consensus_parameters_version;
                let consensus_params = view.storage::<ConsensusParametersVersions>().get(&param_version).map_err(
                    |source_error| GasPriceError::CouldNotFetchL2Block {
                        block_height: latest_height,
                        source_error: source_error.into(),
                    },
                )?.ok_or(GasPriceError::CouldNotFetchL2Block {
                    block_height: latest_height,
                    source_error: anyhow!("Consensus parameters not found in storage: {latest_height:?}",),
                })?;
                let block_gas_limit = consensus_params.block_gas_limit();
                let block_info = get_block_info(&block, block_gas_limit);
                return Ok(block_info);
            }
        }
    }
}
