use anyhow::anyhow;
use fuel_core_gas_price_service::fuel_gas_price_updater::{
    BlockInfo,
    Error as GasPriceError,
    L2BlockSource,
    Result as GasPriceResult,
};
use fuel_core_storage::{
    tables::FuelBlocks,
    transactional::AtomicView,
    StorageAsRef,
    StorageInspect,
};
use fuel_core_types::{
    blockchain::block::CompressedBlock,
    fuel_types::BlockHeight,
};
use std::time::Duration;

#[cfg(test)]
mod tests;

pub struct FuelL2BlockSource<Database> {
    frequency: Duration,
    database: Database,
}

fn get_block_info(block: &CompressedBlock) -> BlockInfo {
    BlockInfo {
        height: (*block.header().height()).into(),
        fullness: (0, 0),
        block_bytes: 0,
        gas_price: 0,
    }
}

#[async_trait::async_trait]
impl<Database> L2BlockSource for FuelL2BlockSource<Database>
where
    Database: AtomicView<Height = BlockHeight>,
    Database::View: StorageAsRef,
    Database::View: StorageInspect<FuelBlocks>,
    <Database::View as StorageInspect<FuelBlocks>>::Error: Into<anyhow::Error>,
{
    async fn get_l2_block(&self, height: BlockHeight) -> GasPriceResult<BlockInfo> {
        // TODO: Add an escape route for loop
        loop {
            let latest_height = self.database.latest_height().unwrap_or(0.into());
            if latest_height < height {
                tokio::time::sleep(self.frequency).await;
            } else {
                let view = self.database.latest_view();
                let _block = view
                    .storage::<FuelBlocks>()
                    .get(&height)
                    .map_err(|source_error| GasPriceError::CouldNotFetchL2Block {
                        block_height: height,
                        source_error: source_error.into(),
                    })?
                    .ok_or(GasPriceError::CouldNotFetchL2Block {
                        block_height: height,
                        source_error: anyhow!(
                            "Block not found in storage dispite being at height {}.",
                            height
                        ),
                    })?;
                return Ok(get_block_info(&_block));
            }
        }
    }
}
