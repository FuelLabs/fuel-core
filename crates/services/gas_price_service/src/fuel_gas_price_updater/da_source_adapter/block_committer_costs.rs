use crate::fuel_gas_price_updater::{
    da_source_adapter::service::{
        DaBlockCostsSource,
        Result as DaBlockCostsResult,
    },
    DaBlockCosts,
};
use anyhow::anyhow;
use reqwest::Url;
use serde::{
    Deserialize,
    Serialize,
};

/// This struct is used to denote the block committer da gas price source,,
/// which receives data from the block committer (only http api for now)
pub struct BlockCommitterDaBlockCosts {
    client: reqwest::Client,
    url: Url,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default, PartialEq)]
struct RawDaBlockCosts {
    pub l2_block_range: core::ops::Range<u32>,
    pub blob_size_bytes: u32,
    pub blob_cost: u128,
}

impl From<RawDaBlockCosts> for DaBlockCosts {
    fn from(raw: RawDaBlockCosts) -> Self {
        DaBlockCosts {
            l2_block_range: raw.l2_block_range,
            blob_size_bytes: raw.blob_size_bytes,
            blob_cost_wei: raw.blob_cost,
        }
    }
}

impl BlockCommitterDaBlockCosts {
    /// Create a new instance of the block committer da gas price source
    pub fn new(url: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            url: Url::parse(&url).unwrap(),
        }
    }
}

#[async_trait::async_trait]
impl DaBlockCostsSource for BlockCommitterDaBlockCosts {
    async fn request_da_block_cost(&mut self) -> DaBlockCostsResult<DaBlockCosts> {
        let response = self.client.get(self.url.clone()).send().await?;
        if !response.status().is_success() {
            return Err(anyhow!("failed with response: {}", response.status()));
        }
        let response = response
            .json::<RawDaBlockCosts>()
            .await
            .map_err(|err| anyhow!(err))?;
        Ok(response.into())
    }
}
