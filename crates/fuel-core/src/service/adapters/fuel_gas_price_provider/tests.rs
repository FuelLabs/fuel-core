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
    gas_prices: HashMap<BlockHeight, u64>,
    production_rewards: HashMap<BlockHeight, u64>,
}

impl FuelBlockHistory for FakeBlockHistory {
    fn latest_height(&self) -> ForeignResult<BlockHeight> {
        Ok(self.latest_height)
    }

    fn gas_price(&self, height: BlockHeight) -> ForeignResult<Option<u64>> {
        Ok(self.gas_prices.get(&height).copied())
    }

    fn block_fullness(
        &self,
        _height: BlockHeight,
    ) -> ForeignResult<Option<BlockFullness>> {
        Ok(Some(BlockFullness))
    }

    fn production_reward(&self, height: BlockHeight) -> ForeignResult<Option<u64>> {
        Ok(self.production_rewards.get(&height).copied())
    }
}

struct FakeDARecordingCostHistory {
    costs: HashMap<BlockHeight, u64>,
}

impl DARecordingCostHistory for FakeDARecordingCostHistory {
    fn recording_cost(&self, height: BlockHeight) -> ForeignResult<Option<u64>> {
        Ok(self.costs.get(&height).copied())
    }
}

pub struct SimpleGasPriceAlgorithm {
    flat_price_change: u64,
    max_price_change: u64,
}

impl SimpleGasPriceAlgorithm {
    pub fn new(flat_price_change: u64, max_price_change: u64) -> Self {
        Self {
            flat_price_change,
            max_price_change,
        }
    }
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
    fn calculate_gas_price(
        &self,
        previous_gas_price: u64,
        total_production_reward: u64,
        total_da_recording_cost: u64,
        _block_fullness: BlockFullness,
    ) -> u64 {
        if total_production_reward < total_da_recording_cost {
            previous_gas_price.saturating_add(self.flat_price_change)
        } else {
            previous_gas_price
        }
    }

    fn maximum_next_gas_price(&self, previous_gas_price: u64) -> u64 {
        previous_gas_price.saturating_add(self.max_price_change)
    }
}

struct ProviderBuilder {
    latest_height: BlockHeight,
    historical_gas_price: HashMap<BlockHeight, u64>,
    production_rewards: HashMap<BlockHeight, u64>,
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
            production_rewards: HashMap::new(),
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
        gas_price: u64,
    ) -> Self {
        self.historical_gas_price.insert(block_height, gas_price);
        self
    }

    fn with_historical_production_reward(
        mut self,
        block_height: BlockHeight,
        reward: u64,
    ) -> Self {
        self.production_rewards.insert(block_height, reward);
        self
    }

    fn with_historicial_da_recording_cost(
        mut self,
        block_height: BlockHeight,
        cost: u64,
    ) -> Self {
        self.da_recording_costs.insert(block_height, cost);
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

    fn with_alorithm_settings(mut self, flat_change: u64, max_change: u64) -> Self {
        self.algorithm = SimpleGasPriceAlgorithm {
            flat_price_change: flat_change,
            max_price_change: max_change,
        };
        self
    }

    fn build(
        self,
    ) -> FuelGasPriceProvider<
        FakeBlockHistory,
        FakeDARecordingCostHistory,
        SimpleGasPriceAlgorithm,
    > {
        let Self {
            latest_height,
            historical_gas_price,
            production_rewards,
            da_recording_costs,
            totaled_block_height,
            total_reward,
            total_cost,
            algorithm,
        } = self;

        let block_history = FakeBlockHistory {
            latest_height,
            gas_prices: historical_gas_price,
            production_rewards,
        };
        let da_recording_cost_history = FakeDARecordingCostHistory {
            costs: da_recording_costs,
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
        }
    }
}

#[ignore]
#[test]
fn dummy() {}
