#![allow(non_snake_case)]
#![allow(clippy::arithmetic_side_effects)]

use crate::v1::da_source_adapter::{
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
    // Used on first run to get the latest costs and seqno
    async fn get_latest_costs(&self) -> DaBlockCostsResult<Option<RawDaBlockCosts>>;
    // Used to get the costs for a specific seqno
    async fn get_costs_by_seqno(
        &self,
        number: u64,
    ) -> DaBlockCostsResult<Option<RawDaBlockCosts>>;
    async fn get_bundles_by_range(
        &self,
        range: core::ops::Range<u64>,
    ) -> DaBlockCostsResult<Vec<Option<RawDaBlockCosts>>>;
}

/// This struct is used to denote the block committer da block costs source
/// which receives data from the block committer (only http api for now)
pub struct BlockCommitterDaBlockCosts<BlockCommitter> {
    client: BlockCommitter,
    last_value: Option<RawDaBlockCosts>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default, PartialEq)]
pub struct RawDaBlockCosts {
    // Sequence number (Monotonically increasing nonce)
    pub sequence_number: u64,
    // The range of blocks that the costs apply to
    pub blocks_range: core::ops::Range<u64>,
    // The DA block height of the last transaction for the range of blocks
    pub da_block_height: DaBlockHeight,
    // Rolling sum cost of posting blobs (wei)
    pub total_cost: u128,
    // Rolling sum size of blobs (bytes)
    pub total_size_bytes: u32,
}

impl<BlockCommitter> BlockCommitterDaBlockCosts<BlockCommitter> {
    /// Create a new instance of the block committer da block costs source
    pub fn new(client: BlockCommitter, last_value: Option<RawDaBlockCosts>) -> Self {
        Self { client, last_value }
    }
}

#[async_trait::async_trait]
impl<BlockCommitter> DaBlockCostsSource for BlockCommitterDaBlockCosts<BlockCommitter>
where
    BlockCommitter: BlockCommitterApi,
{
    async fn request_da_block_cost(&mut self) -> DaBlockCostsResult<DaBlockCosts> {
        let response;

        if let Some(last_value) = &self.last_value {
            response = self
                .client
                .get_costs_by_seqno(last_value.sequence_number + 1)
                .await?;
        } else {
            // we have to error if we cannot find the first set of costs
            response = self.client.get_latest_costs().await?;
        }

        if let Some(response) = response {
            let res;
            if let Some(last_value) = &self.last_value {
                res = DaBlockCosts {
                    l2_block_range: response.blocks_range.clone(),
                    blob_size_bytes: response
                        .total_size_bytes
                        .checked_sub(last_value.total_size_bytes)
                        .ok_or(anyhow!("Blob size bytes underflow"))?,
                    blob_cost_wei: response
                        .total_cost
                        .checked_sub(last_value.total_cost)
                        .ok_or(anyhow!("Blob cost wei underflow"))?,
                };
            } else {
                res = DaBlockCosts {
                    l2_block_range: response.blocks_range.clone(),
                    blob_size_bytes: response.total_size_bytes,
                    blob_cost_wei: response.total_cost,
                };
            }
            self.last_value = Some(response.clone());
            Ok(res)
        } else {
            Err(anyhow!("No response from block committer"))
        }
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
        number: u64,
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

    async fn get_bundles_by_range(
        &self,
        range: core::ops::Range<u64>,
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
            seq_no: u64,
        ) -> DaBlockCostsResult<Option<RawDaBlockCosts>> {
            // arbitrary logic to generate a new value
            let mut value = self.value.clone();
            if let Some(value) = &mut value {
                value.sequence_number = seq_no;
                value.blocks_range =
                    value.blocks_range.end * seq_no..value.blocks_range.end * seq_no + 10;
                value.da_block_height = value.da_block_height + (seq_no + 1).into();
                value.total_cost += 1;
                value.total_size_bytes += 1;
            }
            Ok(value)
        }
        async fn get_bundles_by_range(
            &self,
            _: core::ops::Range<u64>,
        ) -> DaBlockCostsResult<Vec<Option<RawDaBlockCosts>>> {
            Ok(vec![self.value.clone()])
        }
    }

    fn test_da_block_costs() -> RawDaBlockCosts {
        RawDaBlockCosts {
            sequence_number: 1,
            blocks_range: 0..10,
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
        let expected = DaBlockCosts {
            l2_block_range: da_block_costs.blocks_range.clone(),
            blob_size_bytes: da_block_costs.total_size_bytes,
            blob_cost_wei: da_block_costs.total_cost,
        };
        let mock_api = MockBlockCommitterApi::new(Some(da_block_costs.clone()));
        let mut block_committer = BlockCommitterDaBlockCosts::new(mock_api, None);

        // when
        let actual = block_committer.request_da_block_cost().await.unwrap();

        // then
        assert_eq!(actual, expected);
        assert!(block_committer.last_value.is_some());
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
        assert_ne!(da_block_costs.blocks_range, actual.l2_block_range);
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
        assert!(block_committer.last_value.is_none());
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
            seq_no: u64,
        ) -> DaBlockCostsResult<Option<RawDaBlockCosts>> {
            // arbitrary logic to generate a new value
            let mut value = self.value.clone();
            if let Some(value) = &mut value {
                value.sequence_number = seq_no;
                value.blocks_range = value.blocks_range.end..value.blocks_range.end + 10;
                value.da_block_height = value.da_block_height + 1u64.into();
                value.total_cost -= 1;
                value.total_size_bytes -= 1;
            }
            Ok(value)
        }
        async fn get_bundles_by_range(
            &self,
            _: core::ops::Range<u64>,
        ) -> DaBlockCostsResult<Vec<Option<RawDaBlockCosts>>> {
            Ok(vec![self.value.clone()])
        }
    }

    #[tokio::test]
    async fn request_da_block_cost__when_underflow__then_error() {
        // given
        let da_block_costs = test_da_block_costs();
        let expected = DaBlockCosts {
            l2_block_range: da_block_costs.blocks_range.clone(),
            blob_size_bytes: da_block_costs.total_size_bytes,
            blob_cost_wei: da_block_costs.total_cost,
        };
        let mock_api =
            UnderflowingMockBlockCommitterApi::new(Some(da_block_costs.clone()));
        let mut block_committer = BlockCommitterDaBlockCosts::new(mock_api, None);

        // when
        let actual = block_committer.request_da_block_cost().await.unwrap();
        let result = block_committer.request_da_block_cost().await;

        // then
        assert!(result.is_err());
        assert_eq!(actual, expected);
    }
}
