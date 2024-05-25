use crate::service::adapters::fuel_gas_price_provider::ports::{
    BlockFullness,
    GasPriceAlgorithm,
    GasPrices,
};

// TODO: Don't use floats 🤢
pub struct FuelGasPriceAlgorithm {
    _target_profitability: f32,
    _max_price_change_percentage: f32,
}

impl FuelGasPriceAlgorithm {
    pub fn new(target_profitability: f32, max_price_change_percentage: f32) -> Self {
        Self {
            _target_profitability: target_profitability,
            _max_price_change_percentage: max_price_change_percentage,
        }
    }
}

impl GasPriceAlgorithm for FuelGasPriceAlgorithm {
    fn calculate_gas_prices(
        &self,
        previous_gas_prices: GasPrices,
        total_production_reward: u64,
        total_da_recording_cost: u64,
        block_fullness: BlockFullness,
    ) -> GasPrices {
        let GasPrices {
            execution_gas_price,
            da_gas_price,
        } = previous_gas_prices;
        if total_da_recording_cost > total_production_reward {
            GasPrices {
                execution_gas_price,
                da_gas_price: da_gas_price + 1,
            }
        } else {
            if block_fullness.used() > block_fullness.capacity() / 2 {
                GasPrices {
                    execution_gas_price: execution_gas_price + 1,
                    da_gas_price,
                }
            } else {
                GasPrices {
                    execution_gas_price: execution_gas_price - 1,
                    da_gas_price,
                }
            }
        }
    }

    fn maximum_next_gas_prices(&self, _previous_gas_price: GasPrices) -> GasPrices {
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
        let old_gas_prices = GasPrices::new(old_gas_price, 0);
        let total_production_reward = 100;
        let total_da_recording_cost = total_production_reward;
        // 60% full
        let block_fullness = BlockFullness::new(60, 100);
        let algo = FuelGasPriceAlgorithm {
            _target_profitability: 1.,
            _max_price_change_percentage: f32::MAX,
        };
        // when
        let new_gas_price = algo.calculate_gas_prices(
            old_gas_prices,
            total_production_reward,
            total_da_recording_cost,
            block_fullness,
        );

        // then
        assert!(new_gas_price.total() > old_gas_prices.total());
    }

    #[test]
    fn calculate_gas_price__below_50_but_not_profitable_increase_gas_price() {
        // given
        let old_gas_price = 444;
        let old_gas_prices = GasPrices::new(old_gas_price, 0);
        let total_production_reward = 100;
        let total_da_recording_cost = total_production_reward + 1;
        // 40% full
        let block_fullness = BlockFullness::new(40, 100);
        let algo = FuelGasPriceAlgorithm {
            _target_profitability: 1.,
            _max_price_change_percentage: f32::MAX,
        };

        // when
        let new_gas_price = algo.calculate_gas_prices(
            old_gas_prices,
            total_production_reward,
            total_da_recording_cost,
            block_fullness,
        );

        // then
        assert!(new_gas_price.total() > old_gas_prices.total());
    }

    #[test]
    fn calculate_gas_price__below_50_and_profitable_decrease_gas_price() {
        // given
        let old_gas_price = 444;
        let old_gas_prices = GasPrices::new(old_gas_price, 0);
        let total_production_reward = 100;
        let total_da_recording_cost = total_production_reward - 1;
        // 40% full
        let block_fullness = BlockFullness::new(40, 100);
        let algo = FuelGasPriceAlgorithm {
            _target_profitability: 1.,
            _max_price_change_percentage: f32::MAX,
        };

        // when
        let new_gas_prices = algo.calculate_gas_prices(
            old_gas_prices,
            total_production_reward,
            total_da_recording_cost,
            block_fullness,
        );

        // then
        assert!(new_gas_prices.total() < old_gas_price);
    }
}
