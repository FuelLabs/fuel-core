use crate::fuel_gas_price_updater::{
    service::Result as DaGasPriceSourceResult,
    DaGasPrice,
    DaGasPriceSource,
};
use anyhow::anyhow;
use reqwest::Url;
use serde::{
    Deserialize,
    Serialize,
};

/// This struct is used to denote the block committer da gas price source,,
/// which receives data from the block committer (only http api for now)
pub struct BlockCommitterDaGasPriceSource {
    client: reqwest::Client,
    url: Url,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default, PartialEq)]
struct RawDaGasPrice {
    pub l2_block_range: core::ops::Range<u32>,
    pub blob_size_bytes: u32,
    pub blob_cost: u32,
}

impl From<RawDaGasPrice> for DaGasPrice {
    fn from(raw: RawDaGasPrice) -> Self {
        DaGasPrice {
            l2_block_range: raw.l2_block_range,
            blob_size_bytes: raw.blob_size_bytes,
            blob_cost_wei: raw.blob_cost,
        }
    }
}

impl BlockCommitterDaGasPriceSource {
    /// Create a new instance of the block committer da gas price source
    pub fn new(url: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            url: Url::parse(&url).unwrap(),
        }
    }
}

#[async_trait::async_trait]
impl DaGasPriceSource for BlockCommitterDaGasPriceSource {
    async fn get(&mut self) -> DaGasPriceSourceResult<DaGasPrice> {
        let response = self.client.get(self.url.clone()).send().await?;
        if !response.status().is_success() {
            return Err(anyhow!("failed with response: {}", response.status()));
        }
        let response = response
            .json::<RawDaGasPrice>()
            .await
            .map_err(|err| anyhow!(err))?;
        Ok(response.into())
    }
}
