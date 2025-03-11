use std::collections::VecDeque;

use fuel_block_fetcher::BlockFetcher;
use fuel_core_global_merkle_root_service::ports::BlockStream;
use fuel_core_types::blockchain::block::Block;

/// Wrapper around the fuel block fetcher that implements block stream.
pub struct BlockStreamAdapter {
    block_fetcher: BlockFetcher,
    height: u32,
    batch_size: u32,
    blocks: VecDeque<Block>,
}

impl BlockStreamAdapter {
    /// Construct a new block stream adapter.
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

    async fn load_more_blocks(&mut self) -> Result<(), Error> {
        let latest_height = u32::from(self.block_fetcher.last_height().await?);

        let next_height = self
            .height
            .checked_add(self.batch_size)
            .ok_or(Error::BlockHeightOverflow)?
            .min(latest_height)
            .checked_add(1)
            .ok_or(Error::BlockHeightOverflow)?;

        let blocks = self
            .block_fetcher
            .blocks_for(self.height..next_height)
            .await?;

        self.blocks
            .extend(blocks.into_iter().map(|sealed| sealed.block.entity));

        self.height = next_height;

        Ok(())
    }
}

impl BlockStream for BlockStreamAdapter {
    type Error = Error;

    async fn next(&mut self) -> Result<Block, Self::Error> {
        loop {
            match self.blocks.pop_front() {
                Some(block) => break Ok(block),
                None => self.load_more_blocks().await?,
            }
        }
    }
}

/// Block streaming error.
#[derive(Debug, derive_more::Display, derive_more::From)]
pub enum Error {
    /// Integer overflow when incrementing block height.
    BlockHeightOverflow,
    /// Other error.
    #[from]
    Other(anyhow::Error),
}

impl core::error::Error for Error {}
