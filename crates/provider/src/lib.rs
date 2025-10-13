pub use alloy_provider::Provider;
use alloy_provider::{
    RootProvider,
    network::Ethereum,
};
pub struct FuelEthProvider {
    provider: RootProvider<Ethereum>,
}

impl FuelEthProvider {
    pub fn new(url: url::Url) -> Self {
        Self {
            provider: RootProvider::<Ethereum>::new_http(url),
        }
    }
}

impl Provider for FuelEthProvider {
    fn root(&self) -> &RootProvider<Ethereum> {
        &self.provider
    }
}
