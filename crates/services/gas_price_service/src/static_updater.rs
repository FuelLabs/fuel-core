use crate::common::gas_price_algorithm::GasPriceAlgorithm;
use fuel_core_types::fuel_types::BlockHeight;

pub struct StaticAlgorithmUpdater {
    static_price: u64,
}

impl StaticAlgorithmUpdater {
    pub fn new(static_price: u64) -> Self {
        Self { static_price }
    }
}

#[derive(Clone, Debug, Copy)]
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
