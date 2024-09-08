use crate::fuel_gas_price_updater::{
    service::Result as DaGasPriceSourceResult,
    DaGasPriceSource,
    RawDaGasPrice,
};
use anyhow::anyhow;
use reqwest::Url;

/// This struct is used to denote the block committer da gas price source,,
/// which receives data from the block committer (only http api for now)
pub struct BlockCommitterDaGasPriceSource {
    client: reqwest::Client,
    url: Url,
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
    async fn get(&mut self) -> DaGasPriceSourceResult<RawDaGasPrice> {
        let response = self.client.get(self.url.clone()).send().await?;
        if !response.status().is_success() {
            return Err(anyhow!("failed with response: {}", response.status()));
        }
        response
            .json::<RawDaGasPrice>()
            .await
            .map_err(|err| anyhow!(err))
    }
}
