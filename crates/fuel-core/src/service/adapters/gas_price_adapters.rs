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

pub struct FuelL2BlockSource<Onchain> {
    frequency: Duration,
    on_chain: Onchain,
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
impl<Onchain> L2BlockSource for FuelL2BlockSource<Onchain>
where
    Onchain: AtomicView<Height = BlockHeight>,
    Onchain::View: StorageAsRef,
    Onchain::View: StorageInspect<FuelBlocks>,
    <Onchain::View as StorageInspect<FuelBlocks>>::Error: Into<anyhow::Error>,
{
    async fn get_l2_block(&self, height: BlockHeight) -> GasPriceResult<BlockInfo> {
        // TODO: Add an escape route for loop
        loop {
            let latest_height = self.on_chain.latest_height().unwrap_or(0.into());
            if latest_height < height {
                tokio::time::sleep(self.frequency).await;
            } else {
                let view = self.on_chain.latest_view();
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
