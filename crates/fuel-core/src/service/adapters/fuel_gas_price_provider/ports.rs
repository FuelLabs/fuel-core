use fuel_core_types::fuel_types::BlockHeight;

pub trait GasPriceAlgorithm {
    fn gas_price(&self, block_bytes: u64) -> u64;
    fn last_gas_price(&self) -> u64;
}

pub trait GasPriceEstimate {
    fn worst_case_gas_price(&self, block_height: BlockHeight) -> u64;
}
