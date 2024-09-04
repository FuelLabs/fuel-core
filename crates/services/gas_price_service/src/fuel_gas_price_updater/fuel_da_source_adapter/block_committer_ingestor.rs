use crate::fuel_gas_price_updater::fuel_da_source_adapter::service::{
    DaMetadataGetter,
    DaMetadataResponse,
};
use anyhow::anyhow;
use reqwest::Url;

/// This struct is used to denote the block committer ingestor,
/// which receives data from the block committer (only http api for now)
pub struct BlockCommitterIngestor {
    client: reqwest::Client,
    url: Url,
}

impl BlockCommitterIngestor {
    /// Create a new instance of the block committer ingestor
    pub fn new(url: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            url: Url::parse(&url).unwrap(),
        }
    }
}

#[async_trait::async_trait]
impl DaMetadataGetter for BlockCommitterIngestor {
    async fn get_da_metadata(&mut self) -> anyhow::Result<DaMetadataResponse> {
        let response = self.client.get(self.url.clone()).send().await?;
        if !response.status().is_success() {
            return Err(anyhow!("failed with response: {}", response.status()));
        }
        response
            .json::<DaMetadataResponse>()
            .await
            .map_err(|err| anyhow!(err))
    }
}
