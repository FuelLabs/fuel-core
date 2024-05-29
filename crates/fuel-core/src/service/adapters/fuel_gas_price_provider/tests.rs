#![allow(non_snake_case)]

use super::*;
use crate::service::adapters::{
    fuel_gas_price_provider::ports::{
        BlockFullness,
        ForeignResult,
    },
    BlockHeight,
};
use std::collections::HashMap;

#[cfg(test)]
mod producer_gas_price_tests;

struct FakeBlockHistory {
    latest_height: BlockHeight,
    production_rewards: HashMap<BlockHeight, u64>,
    block_fullness: HashMap<BlockHeight, BlockFullness>,
}

impl FuelBlockHistory for FakeBlockHistory {
    fn latest_height(&self) -> ForeignResult<BlockHeight> {
        Ok(self.latest_height)
    }

    fn block_fullness(
        &self,
        height: BlockHeight,
    ) -> ForeignResult<Option<BlockFullness>> {
        Ok(self.block_fullness.get(&height).copied())
    }

    fn production_reward(&self, height: BlockHeight) -> ForeignResult<Option<u64>> {
        Ok(self.production_rewards.get(&height).copied())
    }
}

struct FakeGasPriceHistory {
    gas_prices: RefCell<HashMap<BlockHeight, GasPrices>>,
}

impl GasPriceHistory for FakeGasPriceHistory {
    fn gas_prices(&self, height: BlockHeight) -> ForeignResult<Option<GasPrices>> {
        Ok(self.gas_prices.borrow().get(&height).copied())
    }

    fn store_gas_prices(
        &self,
        height: BlockHeight,
        gas_price: &GasPrices,
    ) -> ForeignResult<()> {
        self.gas_prices
            .borrow_mut()
            .insert(height, gas_price.clone());
        Ok(())
    }
}

struct FakeDARecordingCostHistory {
    costs: HashMap<BlockHeight, u64>,
}

impl DARecordingCostHistory for FakeDARecordingCostHistory {
    fn latest_height(&self) -> ForeignResult<BlockHeight> {
        let max_height = self.costs.iter().map(|(h, _)| *h).max().unwrap_or(0.into());
        Ok(max_height)
    }

    fn recording_cost(&self, height: BlockHeight) -> ForeignResult<Option<u64>> {
        Ok(self.costs.get(&height).copied())
    }
}

pub struct SimpleGasPriceAlgorithm {
    flat_price_change: u64,
    max_price_change: u64,
}

impl Default for SimpleGasPriceAlgorithm {
    fn default() -> Self {
        Self {
            flat_price_change: 1,
            max_price_change: u64::MAX,
        }
    }
}

impl GasPriceAlgorithm for SimpleGasPriceAlgorithm {
    fn calculate_da_gas_price(
        &self,
        previous_da_gas_price: u64,
        total_production_reward: u64,
        total_da_recording_cost: u64,
    ) -> u64 {
        if total_production_reward < total_da_recording_cost {
            previous_da_gas_price.saturating_add(self.flat_price_change)
        } else {
            previous_da_gas_price
        }
    }

    fn calculate_execution_gas_price(
        &self,
        previous_execution_gas_prices: u64,
        _block_fullness: BlockFullness,
    ) -> u64 {
        previous_execution_gas_prices
    }

    fn maximum_next_gas_prices(&self, previous_gas_price: GasPrices) -> GasPrices {
        let GasPrices {
            execution_gas_price,
            da_gas_price,
        } = previous_gas_price;
        let da_gas_price = da_gas_price.saturating_add(self.max_price_change);
        GasPrices {
            execution_gas_price,
            da_gas_price,
        }
    }
}

struct ProviderBuilder {
    latest_height: BlockHeight,
    historical_gas_price: HashMap<BlockHeight, GasPrices>,
    historical_production_rewards: HashMap<BlockHeight, u64>,
    historical_block_fullness: HashMap<BlockHeight, BlockFullness>,
    da_recording_costs: HashMap<BlockHeight, u64>,
    totaled_block_height: BlockHeight,
    total_reward: u64,
    total_cost: u64,
    algorithm: SimpleGasPriceAlgorithm,
}

impl ProviderBuilder {
    fn new() -> Self {
        Self {
            latest_height: 0.into(),
            historical_gas_price: HashMap::new(),
            historical_production_rewards: HashMap::new(),
            historical_block_fullness: HashMap::new(),
            da_recording_costs: HashMap::new(),
            totaled_block_height: 0.into(),
            total_reward: 0,
            total_cost: 0,
            algorithm: SimpleGasPriceAlgorithm {
                flat_price_change: 1,
                max_price_change: u64::MAX,
            },
        }
    }

    fn with_latest_height(mut self, latest_height: BlockHeight) -> Self {
        self.latest_height = latest_height;
        self
    }

    fn with_historical_gas_price(
        mut self,
        block_height: BlockHeight,
        gas_prices: GasPrices,
    ) -> Self {
        self.historical_gas_price.insert(block_height, gas_prices);
        self
    }

    fn with_historical_production_reward(
        mut self,
        block_height: BlockHeight,
        reward: u64,
    ) -> Self {
        self.historical_production_rewards
            .insert(block_height, reward);
        self
    }

    fn with_historical_da_recording_cost(
        mut self,
        block_height: BlockHeight,
        cost: u64,
    ) -> Self {
        self.da_recording_costs.insert(block_height, cost);
        self
    }

    fn with_historical_block_fullness(
        mut self,
        block_height: BlockHeight,
        fullness: BlockFullness,
    ) -> Self {
        self.historical_block_fullness
            .insert(block_height, fullness);
        self
    }

    fn with_total_as_of_block(
        mut self,
        block_height: BlockHeight,
        reward: u64,
        cost: u64,
    ) -> Self {
        self.totaled_block_height = block_height;
        self.total_reward = reward;
        self.total_cost = cost;
        self
    }

    fn build(
        self,
    ) -> FuelGasPriceProvider<
        FakeBlockHistory,
        FakeDARecordingCostHistory,
        SimpleGasPriceAlgorithm,
        FakeGasPriceHistory,
    > {
        let Self {
            latest_height,
            historical_gas_price,
            historical_production_rewards,
            historical_block_fullness,
            da_recording_costs,
            totaled_block_height,
            total_reward,
            total_cost,
            algorithm,
        } = self;

        let block_history = FakeBlockHistory {
            latest_height,
            production_rewards: historical_production_rewards,
            block_fullness: historical_block_fullness,
        };
        let da_recording_cost_history = FakeDARecordingCostHistory {
            costs: da_recording_costs,
        };
        let gas_price_history = FakeGasPriceHistory {
            gas_prices: historical_gas_price.into(),
        };
        FuelGasPriceProvider {
            profitablility_totals: ProfitablilityTotals::new(
                totaled_block_height,
                total_reward,
                total_cost,
            ),
            block_history,
            da_recording_cost_history,
            algorithm,
            gas_price_history,
        }
    }
}

#[ignore]
#[test]
fn dummy() {}
