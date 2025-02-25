use std::collections::VecDeque;

use fuel_block_fetcher::BlockFetcher;
use fuel_core_global_merkle_root_service::ports::BlockStream;
use fuel_core_types::blockchain::block::Block;

pub struct BlockStreamAdapter {
    block_fetcher: BlockFetcher,
    height: u32,
    batch_size: u32,
    blocks: VecDeque<Block>,
}

impl BlockStreamAdapter {
    pub fn new(
        url: impl AsRef<str>,
        height: u32,
        batch_size: u32,
    ) -> anyhow::Result<Self> {
        let block_fetcher = BlockFetcher::new(url)?;
        let blocks = VecDeque::new();

        Ok(Self {
            block_fetcher,
            height,
            batch_size,
            blocks,
        })
    }

    async fn load_more_blocks(&mut self) -> anyhow::Result<()> {
        let latest_height = u32::from(self.block_fetcher.last_height().await?);

        let next_height = self
            .height
            .checked_add(self.batch_size)
            .ok_or_else(|| anyhow::anyhow!("block height overflow"))?
            .min(latest_height)
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("block height overflow"))?;

        let blocks = self
            .block_fetcher
            .blocks_for(self.height..next_height)
            .await?;

        self.blocks
            .extend(blocks.into_iter().map(|sealed| sealed.block.entity));

        Ok(())
    }
}

impl BlockStream for BlockStreamAdapter {
    type Error = anyhow::Error;

    async fn next(&mut self) -> anyhow::Result<Block> {
        loop {
            match self.blocks.pop_front() {
                Some(block) => break Ok(block),
                None => self.load_more_blocks().await?,
            }
        }
    }
}
