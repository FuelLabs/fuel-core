use alloy_provider::{
    network::Ethereum,
    Provider,
    RootProvider,
};
pub struct FuelEthProvider {
    provider: RootProvider<Ethereum>,
}

impl FuelEthProvider {
    fn new(url: Url) -> Self {
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
