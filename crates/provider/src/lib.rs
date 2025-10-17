pub mod quorum;

use alloy_consensus::private::alloy_eips::BlockId;
use alloy_provider::transport::TransportError;
use alloy_provider::{network::Ethereum, EthGetBlock, Provider, RootProvider};
use alloy_rpc_types_eth::Block;
use async_trait::async_trait;

#[async_trait]
pub trait FuelProvider: Send + Sync {
    async fn get_block_number(&self) -> Result<u64, TransportError>;
    async fn get_block(&self, block: BlockId) -> EthGetBlock<Block>;
}

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

#[async_trait]
impl FuelProvider for FuelEthProvider {
    async fn get_block_number(&self) -> Result<u64, TransportError> {
        self.provider.get_block_number().await
    }

    async fn get_block(&self, block: BlockId) -> EthGetBlock<Block> {
        self.provider.get_block(block)
    }
}
