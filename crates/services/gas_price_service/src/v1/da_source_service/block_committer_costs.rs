#![allow(clippy::arithmetic_side_effects)]

use crate::v1::da_source_service::{
    service::{
        DaBlockCostsSource,
        Result as DaBlockCostsResult,
    },
    DaBlockCosts,
};
use anyhow::anyhow;
use fuel_core_types::blockchain::primitives::DaBlockHeight;
use serde::{
    Deserialize,
    Serialize,
};

#[async_trait::async_trait]
trait BlockCommitterApi: Send + Sync {
    /// Used on first run to get the latest costs and seqno
    async fn get_latest_costs(&self) -> DaBlockCostsResult<Option<RawDaBlockCosts>>;
    /// Used to get the costs for a specific seqno
    async fn get_costs_by_seqno(
        &self,
        number: u32,
    ) -> DaBlockCostsResult<Option<RawDaBlockCosts>>;
    /// Used to get the costs for a range of blocks (inclusive)
    async fn get_cost_bundles_by_range(
        &self,
        range: core::ops::Range<u32>,
    ) -> DaBlockCostsResult<Vec<Option<RawDaBlockCosts>>>;
}

/// This struct is used to denote the block committer da block costs source
/// which receives data from the block committer (only http api for now)
pub struct BlockCommitterDaBlockCosts<BlockCommitter> {
    client: BlockCommitter,
    last_raw_da_block_costs: Option<RawDaBlockCosts>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default, PartialEq)]
pub struct RawDaBlockCosts {
    /// Sequence number (Monotonically increasing nonce)
    pub sequence_number: u32,
    /// The range of blocks that the costs apply to
    pub blocks_heights: Vec<u32>,
    /// The DA block height of the last transaction for the range of blocks
    pub da_block_height: DaBlockHeight,
    /// Rolling sum cost of posting blobs (wei)
    pub total_cost: u128,
    /// Rolling sum size of blobs (bytes)
    pub total_size_bytes: u32,
}

impl From<&RawDaBlockCosts> for DaBlockCosts {
    fn from(raw_da_block_costs: &RawDaBlockCosts) -> Self {
        DaBlockCosts {
            l2_blocks: raw_da_block_costs
                .blocks_heights
                .clone()
                .into_iter()
                .collect(),
            blob_size_bytes: raw_da_block_costs.total_size_bytes,
            blob_cost_wei: raw_da_block_costs.total_cost,
        }
    }
}

impl<BlockCommitter> BlockCommitterDaBlockCosts<BlockCommitter> {
    /// Create a new instance of the block committer da block costs source
    pub fn new(client: BlockCommitter, last_value: Option<RawDaBlockCosts>) -> Self {
        Self {
            client,
            last_raw_da_block_costs: last_value,
        }
    }
}

#[async_trait::async_trait]
impl<BlockCommitter> DaBlockCostsSource for BlockCommitterDaBlockCosts<BlockCommitter>
where
    BlockCommitter: BlockCommitterApi,
{
    async fn request_da_block_cost(&mut self) -> DaBlockCostsResult<DaBlockCosts> {
        let raw_da_block_costs = match self.last_raw_da_block_costs {
            Some(ref last_value) => self
                .client
                .get_costs_by_seqno(last_value.sequence_number + 1),
            _ => self.client.get_latest_costs(),
        }
        .await?;

        let Some(ref raw_da_block_costs) = raw_da_block_costs else {
            return Err(anyhow!("No response from block committer"))
        };

        let da_block_costs = self.last_raw_da_block_costs.iter().fold(
            Ok(raw_da_block_costs.into()),
            |costs: DaBlockCostsResult<DaBlockCosts>, last_value| {
                let costs = costs.expect("Defined to be OK");
                let blob_size_bytes = costs
                    .blob_size_bytes
                    .checked_sub(last_value.total_size_bytes)
                    .ok_or(anyhow!("Blob size bytes underflow"))?;
                let blob_cost_wei = raw_da_block_costs
                    .total_cost
                    .checked_sub(last_value.total_cost)
                    .ok_or(anyhow!("Blob cost wei underflow"))?;
                Ok(DaBlockCosts {
                    blob_size_bytes,
                    blob_cost_wei,
                    ..costs
                })
            },
        )?;

        self.last_raw_da_block_costs = Some(raw_da_block_costs.clone());
        Ok(da_block_costs)
    }
}

pub struct BlockCommitterHttpApi {
    client: reqwest::Client,
    url: String,
}

impl BlockCommitterHttpApi {
    pub fn new(url: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            url,
        }
    }
}

#[async_trait::async_trait]
impl BlockCommitterApi for BlockCommitterHttpApi {
    async fn get_latest_costs(&self) -> DaBlockCostsResult<Option<RawDaBlockCosts>> {
        let response = self
            .client
            .get(&self.url)
            .send()
            .await?
            .json::<RawDaBlockCosts>()
            .await?;
        Ok(Some(response))
    }

    async fn get_costs_by_seqno(
        &self,
        number: u32,
    ) -> DaBlockCostsResult<Option<RawDaBlockCosts>> {
        let response = self
            .client
            .get(&format!("{}/{}", self.url, number))
            .send()
            .await?
            .json::<RawDaBlockCosts>()
            .await?;
        Ok(Some(response))
    }

    async fn get_cost_bundles_by_range(
        &self,
        range: core::ops::Range<u32>,
    ) -> DaBlockCostsResult<Vec<Option<RawDaBlockCosts>>> {
        let response = self
            .client
            .get(&format!("{}/{}-{}", self.url, range.start, range.end))
            .send()
            .await?
            .json::<Vec<RawDaBlockCosts>>()
            .await?;
        Ok(response.into_iter().map(Some).collect())
    }
}

#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use super::*;

    struct MockBlockCommitterApi {
        value: Option<RawDaBlockCosts>,
    }

    impl MockBlockCommitterApi {
        fn new(value: Option<RawDaBlockCosts>) -> Self {
            Self { value }
        }
    }

    #[async_trait::async_trait]
    impl BlockCommitterApi for MockBlockCommitterApi {
        async fn get_latest_costs(&self) -> DaBlockCostsResult<Option<RawDaBlockCosts>> {
            Ok(self.value.clone())
        }
        async fn get_costs_by_seqno(
            &self,
            seq_no: u32,
        ) -> DaBlockCostsResult<Option<RawDaBlockCosts>> {
            // arbitrary logic to generate a new value
            let mut value = self.value.clone();
            if let Some(value) = &mut value {
                value.sequence_number = seq_no;
                value.blocks_heights = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]
                    .to_vec()
                    .iter()
                    .map(|x| x * seq_no)
                    .collect();
                value.da_block_height =
                    value.da_block_height + ((seq_no + 1) as u64).into();
                value.total_cost += 1;
                value.total_size_bytes += 1;
            }
            Ok(value)
        }
        async fn get_cost_bundles_by_range(
            &self,
            _: core::ops::Range<u32>,
        ) -> DaBlockCostsResult<Vec<Option<RawDaBlockCosts>>> {
            Ok(vec![self.value.clone()])
        }
    }

    fn test_da_block_costs() -> RawDaBlockCosts {
        RawDaBlockCosts {
            sequence_number: 1,
            blocks_heights: (0..10).collect(),
            da_block_height: 1u64.into(),
            total_cost: 1,
            total_size_bytes: 1,
        }
    }

    #[tokio::test]
    async fn request_da_block_cost__when_last_value_is_none__then_get_latest_costs_is_called(
    ) {
        // given
        let da_block_costs = test_da_block_costs();
        let expected = (&da_block_costs).into();
        let mock_api = MockBlockCommitterApi::new(Some(da_block_costs));
        let mut block_committer = BlockCommitterDaBlockCosts::new(mock_api, None);

        // when
        let actual = block_committer.request_da_block_cost().await.unwrap();

        // then
        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn request_da_block_cost__when_last_value_is_some__then_get_costs_by_seqno_is_called(
    ) {
        // given
        let mut da_block_costs = test_da_block_costs();
        let mock_api = MockBlockCommitterApi::new(Some(da_block_costs.clone()));
        let mut block_committer =
            BlockCommitterDaBlockCosts::new(mock_api, Some(da_block_costs.clone()));

        // when
        let actual = block_committer.request_da_block_cost().await.unwrap();

        // then
        assert_ne!(da_block_costs.blocks_heights, actual.l2_blocks);
    }

    #[tokio::test]
    async fn request_da_block_cost__when_response_is_none__then_error() {
        // given
        let mock_api = MockBlockCommitterApi::new(None);
        let mut block_committer = BlockCommitterDaBlockCosts::new(mock_api, None);

        // when
        let result = block_committer.request_da_block_cost().await;

        // then
        assert!(result.is_err());
    }

    struct UnderflowingMockBlockCommitterApi {
        value: Option<RawDaBlockCosts>,
    }

    impl UnderflowingMockBlockCommitterApi {
        fn new(value: Option<RawDaBlockCosts>) -> Self {
            Self { value }
        }
    }

    #[async_trait::async_trait]
    impl BlockCommitterApi for UnderflowingMockBlockCommitterApi {
        async fn get_latest_costs(&self) -> DaBlockCostsResult<Option<RawDaBlockCosts>> {
            Ok(self.value.clone())
        }
        async fn get_costs_by_seqno(
            &self,
            seq_no: u32,
        ) -> DaBlockCostsResult<Option<RawDaBlockCosts>> {
            // arbitrary logic to generate a new value
            let mut value = self.value.clone();
            if let Some(value) = &mut value {
                value.sequence_number = seq_no;
                value.blocks_heights =
                    value.blocks_heights.iter().map(|x| x + seq_no).collect();
                value.da_block_height = value.da_block_height + 1u64.into();
                value.total_cost -= 1;
                value.total_size_bytes -= 1;
            }
            Ok(value)
        }
        async fn get_cost_bundles_by_range(
            &self,
            _: core::ops::Range<u32>,
        ) -> DaBlockCostsResult<Vec<Option<RawDaBlockCosts>>> {
            Ok(vec![self.value.clone()])
        }
    }

    #[tokio::test]
    async fn request_da_block_cost__when_underflow__then_error() {
        // given
        let da_block_costs = test_da_block_costs();
        let mock_api = UnderflowingMockBlockCommitterApi::new(Some(da_block_costs));
        let mut block_committer = BlockCommitterDaBlockCosts::new(mock_api, None);
        let _ = block_committer.request_da_block_cost().await.unwrap();

        // when
        let result = block_committer.request_da_block_cost().await;

        // then
        assert!(result.is_err());
    }
}
