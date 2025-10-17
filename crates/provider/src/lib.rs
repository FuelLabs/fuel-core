pub use alloy_provider::Provider;
pub mod quorum;
use alloy_provider::{
    network::Ethereum,
    RootProvider,
};

//#[cfg(any(test, feature = "test-helpers"))]
pub mod test_helpers;

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
