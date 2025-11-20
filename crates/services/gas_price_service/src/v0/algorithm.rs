use crate::common::gas_price_algorithm::{GasPriceAlgorithm, SharedGasPriceAlgo};
use fuel_core_types::fuel_types::BlockHeight;
use fuel_gas_price_algorithm::v0::AlgorithmV0;

impl GasPriceAlgorithm for AlgorithmV0 {
    fn next_gas_price(&self) -> u64 {
        self.calculate()
    }

    fn worst_case_gas_price(&self, block_height: BlockHeight) -> u64 {
        self.worst_case(block_height.into())
    }
}

pub type SharedV0Algorithm = SharedGasPriceAlgo<AlgorithmV0>;
