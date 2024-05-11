use crate::service::adapters::fuel_gas_price_provider::ports::{
    BlockFullness,
    GasPriceAlgorithm,
};

pub struct FuelGasPriceAlgorithm {
    _target_profitability: f32,
    _max_price_change_percentage: f32,
}

impl GasPriceAlgorithm for FuelGasPriceAlgorithm {
    fn calculate_gas_price(
        &self,
        _previous_gas_price: u64,
        _total_production_reward: u64,
        _total_da_recording_cost: u64,
        _block_fullness: BlockFullness,
    ) -> u64 {
        todo!()
    }

    fn maximum_next_gas_price(&self, _previous_gas_price: u64) -> u64 {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    #![allow(non_snake_case)]
    // use super::*;

    #[test]
    fn calculate_gas_price__above_50_percent_increases_gas_price() {
        todo!()
    }

    #[test]
    fn calculate_gas_price__below_50_but_not_profitable_increase_gas_price() {
        todo!()
    }

    #[test]
    fn calculate_gas_price__below_50_and_profitable_decrease_gas_price() {
        todo!()
    }
}
