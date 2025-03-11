#![deny(unused_crate_dependencies)]

use fuel_core_client::client::{
    pagination::{
        PageDirection,
        PaginationRequest,
    },
    FuelClient,
};
use fuel_core_client_block_query_extension::{
    ClientExt,
    SealedBlockWithMetadata,
};
use fuel_core_compression::VersionedCompressedBlock;
use fuel_core_types::fuel_types::BlockHeight;
use itertools::Itertools;
use std::ops::Range;

#[derive(Clone)]
pub struct BlockFetcher {
    client: FuelClient,
}

impl BlockFetcher {
    pub fn new(url: impl AsRef<str>) -> anyhow::Result<Self> {
        let client = FuelClient::new(url)?;
        Ok(Self { client })
    }
}

impl BlockFetcher {
    pub async fn last_height(&self) -> anyhow::Result<BlockHeight> {
        let chain_info = self.client.chain_info().await?;
        let height = chain_info.latest_block.header.height.into();

        Ok(height)
    }

    pub async fn blocks_for(
        &self,
        range: Range<u32>,
    ) -> anyhow::Result<Vec<SealedBlockWithMetadata>> {
        if range.is_empty() {
            return Ok(vec![]);
        }

        let start = range.start.saturating_sub(1);
        let size = i32::try_from(range.len()).expect("Should be a valid i32");

        let request = PaginationRequest {
            cursor: Some(start.to_string()),
            results: size,
            direction: PageDirection::Forward,
        };
        let response = self.client.full_blocks(request).await?;
        let blocks = response
            .results
            .into_iter()
            .map(TryInto::try_into)
            .try_collect()?;
        Ok(blocks)
    }

    pub async fn compressed_blocks_for(
        &self,
        range: Range<u32>,
    ) -> anyhow::Result<Vec<Option<VersionedCompressedBlock>>> {
        if range.is_empty() {
            return Ok(vec![]);
        }

        let futures = range
            .into_iter()
            .map(|i| {
                let block_height: BlockHeight = i.into();
                self.client.da_compressed_block(block_height)
            })
            .collect::<Vec<_>>();

        let compressed_blocks = futures::future::try_join_all(futures).await?;

        let compressed_blocks = compressed_blocks
            .into_iter()
            .map(|block| block.map(|block| postcard::from_bytes(&block)).transpose())
            .try_collect()?;

        Ok(compressed_blocks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[ignore]
    #[tokio::test]
    async fn testnet_works() {
        let syncer = BlockFetcher::new("https://testnet.fuel.network")
            .expect("Should connect to the testnet network");
        // Given
        const START: u32 = 77;
        const END: u32 = 110;

        // When
        let result = syncer.blocks_for(START..END).await;

        // Then
        let blocks = result.expect("Should get blocks");
        assert_eq!(blocks.len(), (END - START) as usize);

        for i in START..END {
            let block = &blocks[i.saturating_sub(START) as usize];
            assert_eq!(*block.block.entity.header().height(), i.into());
        }
    }

    #[ignore]
    #[tokio::test]
    async fn compressed_blocks_works() {
        let syncer = BlockFetcher::new("http://127.0.0.1:4000")
            .expect("Should connect to local network");

        // Given
        const TO_SYNC: u32 = 40;
        let end: u32 = syncer.last_height().await.unwrap().into();
        let start = end.saturating_sub(TO_SYNC);

        // When
        let result = syncer.compressed_blocks_for(start..end).await;

        // Then
        let blocks = result.expect("Should get blocks");
        assert_eq!(blocks.len(), (end - start) as usize);

        for i in start..end {
            let Some(block) = &blocks[i.saturating_sub(start) as usize] else {
                panic!("Block at height {} is missing", i);
            };

            let block = match block {
                VersionedCompressedBlock::V0(block) => block,
                #[cfg(feature = "fault-proving")]
                _ => panic!("unexpected block version"),
            };

            assert_eq!(*block.header.height(), i.into());
        }
    }
}
