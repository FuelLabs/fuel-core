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
pub trait BlockCommitterApi: Send + Sync {
    /// Used on first run to get the latest costs and seqno
    async fn get_latest_costs(&self) -> DaBlockCostsResult<Option<RawDaBlockCosts>>;
    /// Used to get the costs for a specific seqno
    async fn get_costs_by_l2_block_number(
        &self,
        l2_block_number: u32,
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
    pub bundle_id: u32,
    /// The beginning of the range of blocks that the costs apply to
    pub start_height: u32,
    /// The end of the range of blocks that the costs apply to
    pub end_height: u32,
    /// The DA block height of the last transaction for the range of blocks
    pub da_block_height: DaBlockHeight,
    /// cost of posting this blob (wei)
    pub cost_wei: u128,
    /// size of this blob (bytes)
    pub size_bytes: u32,
}

impl From<&RawDaBlockCosts> for DaBlockCosts {
    fn from(raw_da_block_costs: &RawDaBlockCosts) -> Self {
        let RawDaBlockCosts {
            start_height,
            end_height,
            da_block_height,
            cost_wei,
            size_bytes,
            bundle_id,
        } = *raw_da_block_costs;
        DaBlockCosts {
            bundle_id,
            // construct a vec of l2 blocks from the start_height to the end_height
            l2_blocks: (start_height..end_height).collect(),
            bundle_size_bytes: raw_da_block_costs.size_bytes,
            blob_cost_wei: raw_da_block_costs.cost_wei,
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
    async fn request_da_block_cost(
        &mut self,
    ) -> DaBlockCostsResult<Option<DaBlockCosts>> {
        let raw_da_block_costs = match self.last_raw_da_block_costs {
            Some(ref last_value) => self
                .client
                .get_costs_by_l2_block_number(last_value.end_height + 1),
            _ => self.client.get_latest_costs(),
        }
        .await?;

        let Some(ref raw_da_block_costs) = raw_da_block_costs else {
            // TODO: This is really annoying if there haven't been any costs yet. Do we need this?
            //   Gonna return `Option::None` for now
            // return Err(anyhow!("No response from block committer"))
            return Ok(None)
        };

        let da_block_costs = self.last_raw_da_block_costs.iter().fold(
            Ok(raw_da_block_costs.into()),
            |costs: DaBlockCostsResult<DaBlockCosts>, last_value| {
                let costs = costs.expect("Defined to be OK");
                let blob_size_bytes = costs
                    .bundle_size_bytes
                    .checked_sub(last_value.size_bytes)
                    .ok_or(anyhow!("Blob size bytes underflow"))?;
                let blob_cost_wei = raw_da_block_costs
                    .cost_wei
                    .checked_sub(last_value.cost_wei)
                    .ok_or(anyhow!("Blob cost wei underflow"))?;
                Ok(DaBlockCosts {
                    bundle_size_bytes: blob_size_bytes,
                    blob_cost_wei,
                    ..costs
                })
            },
        )?;

        self.last_raw_da_block_costs = Some(raw_da_block_costs.clone());
        Ok(Some(da_block_costs))
    }

    async fn set_last_value(&mut self, bundle_id: u32) -> DaBlockCostsResult<()> {
        self.last_raw_da_block_costs =
            self.client.get_costs_by_l2_block_number(bundle_id).await?;
        Ok(())
    }
}

pub struct BlockCommitterHttpApi {
    client: reqwest::Client,
    url: Option<String>,
}

impl BlockCommitterHttpApi {
    pub fn new(url: Option<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            url,
        }
    }
}

#[async_trait::async_trait]
impl BlockCommitterApi for BlockCommitterHttpApi {
    async fn get_latest_costs(&self) -> DaBlockCostsResult<Option<RawDaBlockCosts>> {
        if let Some(url) = &self.url {
            let val = self.client.get(url).send().await?;
            tracing::warn!("val: {:?}", val);
            let response = val.json::<Option<RawDaBlockCosts>>().await?;
            tracing::warn!("Response: {:?}", response);
            Ok(response)
        } else {
            Ok(None)
        }
    }

    async fn get_costs_by_l2_block_number(
        &self,
        l2_block_number: u32,
    ) -> DaBlockCostsResult<Option<RawDaBlockCosts>> {
        if let Some(url) = &self.url {
            let val = self
                .client
                .get(format!("{url}/v1/costs?from_height={l2_block_number}"))
                .send()
                .await?;
            tracing::warn!("val: {:?}", val);
            let response = val.json::<Option<RawDaBlockCosts>>().await?;
            tracing::warn!("Response: {:?}", response);
            Ok(response)
        } else {
            Ok(None)
        }
    }

    async fn get_cost_bundles_by_range(
        &self,
        range: core::ops::Range<u32>,
    ) -> DaBlockCostsResult<Vec<Option<RawDaBlockCosts>>> {
        let start = range.start;
        let range_len = range.len();

        if let Some(url) = &self.url {
            let response = self
                .client
                .get(format!(
                    "{url}/v1/costs?from_height={start}&limit={range_len}"
                ))
                .send()
                .await?
                .json::<Vec<RawDaBlockCosts>>()
                .await?;
            Ok(response.into_iter().map(Some).collect())
        } else {
            Ok(vec![])
        }
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
        async fn get_costs_by_l2_block_number(
            &self,
            l2_block_number: u32,
        ) -> DaBlockCostsResult<Option<RawDaBlockCosts>> {
            // arbitrary logic to generate a new value
            let mut value = self.value.clone();
            if let Some(value) = &mut value {
                value.start_height = l2_block_number;
                value.end_height = value.end_height + l2_block_number + 10;
                value.da_block_height =
                    value.da_block_height + ((l2_block_number + 1) as u64).into();
                value.cost_wei += 1;
                value.size_bytes += 1;
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
            bundle_id: 1,
            start_height: 1,
            end_height: 10,
            da_block_height: 1u64.into(),
            cost_wei: 1,
            size_bytes: 1,
        }
    }

    #[tokio::test]
    async fn request_da_block_cost__when_last_value_is_none__then_get_latest_costs_is_called(
    ) {
        // given
        let da_block_costs = test_da_block_costs();
        let expected = Some((&da_block_costs).into());
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
        let da_block_costs_len = da_block_costs.end_height - da_block_costs.start_height;
        let mock_api = MockBlockCommitterApi::new(Some(da_block_costs.clone()));
        let mut block_committer =
            BlockCommitterDaBlockCosts::new(mock_api, Some(da_block_costs.clone()));

        // when
        let actual = block_committer.request_da_block_cost().await.unwrap();

        // then
        assert_ne!(da_block_costs_len as usize, actual.unwrap().l2_blocks.len());
    }

    // TODO: Do we need this?
    // #[tokio::test]
    // async fn request_da_block_cost__when_response_is_none__then_error() {
    //     // given
    //     let mock_api = MockBlockCommitterApi::new(None);
    //     let mut block_committer = BlockCommitterDaBlockCosts::new(mock_api, None);
    //
    //     // when
    //     let result = block_committer.request_da_block_cost().await;
    //
    //     // then
    //     assert!(result.is_err());
    // }

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
        async fn get_costs_by_l2_block_number(
            &self,
            l2_block_number: u32,
        ) -> DaBlockCostsResult<Option<RawDaBlockCosts>> {
            // arbitrary logic to generate a new value
            let mut value = self.value.clone();
            if let Some(value) = &mut value {
                value.start_height = l2_block_number;
                value.end_height = value.end_height + l2_block_number + 10;
                value.da_block_height = value.da_block_height + 1u64.into();
                value.cost_wei -= 1;
                value.size_bytes -= 1;
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
