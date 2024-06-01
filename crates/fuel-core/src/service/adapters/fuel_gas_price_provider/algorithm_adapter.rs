use crate::service::adapters::fuel_gas_price_provider::ports::{
    BlockFullness,
    GasPriceAlgorithm,
    GasPrices,
};
use gas_price_algorithm::AlgorithmV1;

pub enum FuelGasPriceAlgorithm {
    V1(AlgorithmV1),
}

impl FuelGasPriceAlgorithm {
    pub fn new(_target_profitability: f32, _max_price_change_percentage: f32) -> Self {
        todo!()
    }
}

impl GasPriceAlgorithm for FuelGasPriceAlgorithm {
    fn calculate_da_gas_price(
        &self,
        previous_da_gas_prices: u64,
        total_production_reward: u64,
        total_da_recording_cost: u64,
    ) -> u64 {
        match self {
            FuelGasPriceAlgorithm::V1(v1) => v1.calculate_da_gas_price(
                previous_da_gas_prices,
                total_production_reward,
                total_da_recording_cost,
            ),
        }
    }

    fn calculate_execution_gas_price(
        &self,
        previous_execution_gas_price: u64,
        block_fullness: BlockFullness,
    ) -> u64 {
        match self {
            FuelGasPriceAlgorithm::V1(v1) => v1.calculate_exec_gas_price(
                previous_execution_gas_price,
                block_fullness.used(),
                block_fullness.capacity(),
            ),
        }
    }

    fn maximum_next_gas_prices(&self, _previous_gas_price: GasPrices) -> GasPrices {
        todo!()
    }
}

// TODO: These tests need to be updated. Also the specific behavior should probably be managed by
//     the specific algorithm _version_ we are using.
#[cfg(test)]
mod tests {
    #![allow(non_snake_case)]
    use super::*;

    #[test]
    fn calculate_gas_price__above_50_percent_increases_gas_price() {
        // given
        let old_da_gas_price = 444;
        let old_exec_gas_price = 333;
        let total_production_reward = 100;
        let total_da_recording_cost = total_production_reward;
        let min_da_price = 10;
        let min_exec_price = 10;
        let p_value_factor = 4_000;
        let d_value_factor = 100;
        let moving_average_window = 10;
        let max_change_percent = 15;
        let exec_change_amount = 10;
        // 60% full
        let block_fullness = BlockFullness::new(60, 100);
        let v1 = AlgorithmV1::new(
            min_da_price,
            min_exec_price,
            p_value_factor,
            d_value_factor,
            moving_average_window,
            max_change_percent,
            exec_change_amount,
        );
        let algo = FuelGasPriceAlgorithm::V1(v1);
        // when
        let new_da_gas_price = algo.calculate_da_gas_price(
            old_da_gas_price,
            total_production_reward,
            total_da_recording_cost,
        );
        let new_exec_gas_price =
            algo.calculate_execution_gas_price(old_exec_gas_price, block_fullness);
        let new_gas_price = new_exec_gas_price + new_da_gas_price;
        let old_gas_price = old_exec_gas_price + old_da_gas_price;

        // then
        assert!(new_gas_price > old_gas_price);
    }

    #[test]
    fn calculate_gas_price__below_50_but_not_profitable_increase_gas_price() {
        // given
        let old_da_gas_price = 444;
        let old_exec_gas_price = 333;
        let total_production_reward = 100;
        let total_da_recording_cost = total_production_reward + 1;
        let min_da_price = 10;
        let min_exec_price = 10;
        let p_value_factor = 4_000;
        let d_value_factor = 100;
        let moving_average_window = 10;
        let max_change_percent = 15;
        let exec_change_amount = 10;
        // 40% full
        let block_fullness = BlockFullness::new(40, 100);
        let v1 = AlgorithmV1::new(
            min_da_price,
            min_exec_price,
            p_value_factor,
            d_value_factor,
            moving_average_window,
            max_change_percent,
            exec_change_amount,
        );
        let algo = FuelGasPriceAlgorithm::V1(v1);

        // when
        let new_da_gas_price = algo.calculate_da_gas_price(
            old_da_gas_price,
            total_production_reward,
            total_da_recording_cost,
        );
        let new_exec_gas_price =
            algo.calculate_execution_gas_price(old_exec_gas_price, block_fullness);
        let new_gas_price = new_exec_gas_price + new_da_gas_price;
        let old_gas_price = old_exec_gas_price + old_da_gas_price;

        // then
        assert!(new_gas_price > old_gas_price);
    }

    #[test]
    fn calculate_gas_price__below_50_and_profitable_decrease_gas_price() {
        // given
        let old_da_gas_price = 444;
        let old_exec_gas_price = 333;
        let total_production_reward = 100;
        let total_da_recording_cost = total_production_reward - 1;
        let min_da_price = 10;
        let min_exec_price = 10;
        let p_value_factor = 4_000;
        let d_value_factor = 100;
        let moving_average_window = 10;
        let max_change_percent = 15;
        let exec_change_amount = 10;
        // 40% full
        let block_fullness = BlockFullness::new(40, 100);
        let v1 = AlgorithmV1::new(
            min_da_price,
            min_exec_price,
            p_value_factor,
            d_value_factor,
            moving_average_window,
            max_change_percent,
            exec_change_amount,
        );
        let algo = FuelGasPriceAlgorithm::V1(v1);

        // when
        let new_da_gas_price = algo.calculate_da_gas_price(
            old_da_gas_price,
            total_production_reward,
            total_da_recording_cost,
        );
        let new_exec_gas_price =
            algo.calculate_execution_gas_price(old_exec_gas_price, block_fullness);
        let new_gas_price = new_exec_gas_price + new_da_gas_price;
        let old_gas_price = old_exec_gas_price + old_da_gas_price;

        // then
        assert!(new_gas_price < old_gas_price);
    }
}
