use crate::service::adapters::fuel_gas_price_provider::ports::{
    BlockFullness,
    GasPriceAlgorithm,
};

// TODO: Don't use floats 🤢
pub struct FuelGasPriceAlgorithm {
    target_profitability: f32,
    max_price_change_percentage: f32,
}

impl FuelGasPriceAlgorithm {
    pub fn new(target_profitability: f32, max_price_change_percentage: f32) -> Self {
        Self {
            target_profitability,
            max_price_change_percentage,
        }
    }
}

impl GasPriceAlgorithm for FuelGasPriceAlgorithm {
    fn calculate_gas_price(
        &self,
        previous_gas_price: u64,
        total_production_reward: u64,
        total_da_recording_cost: u64,
        block_fullness: BlockFullness,
    ) -> u64 {
        if total_da_recording_cost > total_production_reward {
            previous_gas_price + 1
        } else {
            if block_fullness.used() > block_fullness.capacity() / 2 {
                previous_gas_price + 1
            } else {
                previous_gas_price - 1
            }
        }
    }

    fn maximum_next_gas_price(&self, _previous_gas_price: u64) -> u64 {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    #![allow(non_snake_case)]
    use super::*;

    use criterion as _;

    #[test]
    fn calculate_gas_price__above_50_percent_increases_gas_price() {
        // given
        let old_gas_price = 444;
        let total_production_reward = 100;
        let total_da_recording_cost = total_production_reward;
        // 60% full
        let block_fullness = BlockFullness::new(60, 100);
        let algo = FuelGasPriceAlgorithm {
            target_profitability: 1.,
            max_price_change_percentage: f32::MAX,
        };
        // when
        let new_gas_price = algo.calculate_gas_price(
            old_gas_price,
            total_production_reward,
            total_da_recording_cost,
            block_fullness,
        );

        // then
        assert!(new_gas_price > old_gas_price);
    }

    #[test]
    fn calculate_gas_price__below_50_but_not_profitable_increase_gas_price() {
        // given
        let old_gas_price = 444;
        let total_production_reward = 100;
        let total_da_recording_cost = total_production_reward + 1;
        // 40% full
        let block_fullness = BlockFullness::new(40, 100);
        let algo = FuelGasPriceAlgorithm {
            target_profitability: 1.,
            max_price_change_percentage: f32::MAX,
        };

        // when
        let new_gas_price = algo.calculate_gas_price(
            old_gas_price,
            total_production_reward,
            total_da_recording_cost,
            block_fullness,
        );

        // then
        assert!(new_gas_price > old_gas_price);
    }

    #[test]
    fn calculate_gas_price__below_50_and_profitable_decrease_gas_price() {
        // given
        let old_gas_price = 444;
        let total_production_reward = 100;
        let total_da_recording_cost = total_production_reward - 1;
        // 40% full
        let block_fullness = BlockFullness::new(40, 100);
        let algo = FuelGasPriceAlgorithm {
            target_profitability: 1.,
            max_price_change_percentage: f32::MAX,
        };

        // when
        let new_gas_price = algo.calculate_gas_price(
            old_gas_price,
            total_production_reward,
            total_da_recording_cost,
            block_fullness,
        );

        // then
        assert!(new_gas_price < old_gas_price);
    }
}
