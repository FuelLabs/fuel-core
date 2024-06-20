use crate::fuel_gas_price_updater::{
    BlockInfo,
    L2BlockSource,
};
use fuel_core_storage::transactional::AtomicView;
use fuel_core_types::fuel_types::BlockHeight;
use std::time::Duration;

pub struct FuelL2BlockSource<Onchain> {
    frequency: Duration,
}

trait DatabaseBlocks {
    fn latest_block(&self) -> Result<BlockInfo>;
    fn block(&self, height: BlockHeight) -> Result<BlockInfo>;
}

impl DatabaseBlocks for Database {}

impl<Onchain> L2BlockSource for FuelL2BlockSource<Onchain>
where
    Onchain: AtomicView<Height = BlockHeight>,
    Onchain::View: DatabaseBlocks,
{
    async fn get_l2_block(
        &self,
        height: BlockHeight,
    ) -> crate::fuel_gas_price_updater::Result<BlockInfo> {
        loop {
            let latest_block = self.on_chain().latest_block()?;
            if latest_block.header().height() >= height {
                tokio::time::sleep(self.frequency).await;
            } else {
                let block = self.on_chain().block(height)?;
                todo!();
                return Ok(BlockInfo {
                    height: block.header().height(),
                    fullness: (0, 0),
                    block_bytes: 0,
                    gas_price: 0,
                });
            }
        }
    }
}
