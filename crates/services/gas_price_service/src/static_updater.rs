use crate::{
    GasPriceAlgorithm,
    UpdateAlgorithm,
};
use async_trait::async_trait;
use fuel_core_types::fuel_types::BlockHeight;

pub struct StaticAlgorithmUpdater {
    static_price: u64,
}

impl StaticAlgorithmUpdater {
    pub fn new(static_price: u64) -> Self {
        Self { static_price }
    }
}

#[derive(Clone, Debug)]
pub struct StaticAlgorithm {
    price: u64,
}

impl StaticAlgorithm {
    pub fn new(price: u64) -> Self {
        Self { price }
    }

    pub fn price(&self) -> u64 {
        self.price
    }
}

impl GasPriceAlgorithm for StaticAlgorithm {
    fn next_gas_price(&self) -> u64 {
        self.price()
    }

    fn worst_case_gas_price(&self, _block_height: BlockHeight) -> u64 {
        self.price()
    }
}
#[async_trait]
impl UpdateAlgorithm for StaticAlgorithmUpdater {
    type Algorithm = StaticAlgorithm;

    fn start(&self, _for_block: BlockHeight) -> Self::Algorithm {
        StaticAlgorithm::new(self.static_price)
    }

    async fn next(&mut self) -> anyhow::Result<Self::Algorithm> {
        futures::future::pending().await
    }
}
