#![allow(non_snake_case)]

use super::*;
use crate::service::adapters::{
    fuel_gas_price_provider::ports::BlockFullness,
    BlockHeight,
};
use std::collections::HashMap;

#[cfg(test)]
mod producer_gas_price_tests;

struct FakeBlockHistory {
    latest_height: BlockHeight,
    history: HashMap<BlockHeight, u64>,
}

impl FuelBlockHistory for FakeBlockHistory {
    fn latest_height(&self) -> BlockHeight {
        self.latest_height
    }

    fn gas_price(&self, height: BlockHeight) -> Option<u64> {
        self.history.get(&height).copied()
    }

    fn block_fullness(&self, _height: BlockHeight) -> Option<BlockFullness> {
        Some(BlockFullness)
    }
}

struct FakeDARecordingCostHistory;

impl DARecordingCostHistory for FakeDARecordingCostHistory {
    fn recording_cost(&self, _height: BlockHeight) -> Option<u64> {
        todo!();
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
    totalled_block: BlockHeight,
    total_reward: u64,
    total_cost: u64,
    algorithm: SimpleGasPriceAlgorithm,
}

impl ProviderBuilder {
    fn new() -> Self {
        Self {
            latest_height: 0.into(),
            historical_gas_price: HashMap::new(),
            totalled_block: 0.into(),
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

    fn with_total_as_of_block(
        mut self,
        block_height: BlockHeight,
        reward: u64,
        cost: u64,
    ) -> Self {
        self.totalled_block = block_height;
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
            totalled_block,
            total_reward,
            total_cost,
            algorithm,
        } = self;

        let fake_block_history = FakeBlockHistory {
            latest_height,
            history: historical_gas_price,
        };
        FuelGasPriceProvider {
            totaled_block_height: totalled_block,
            total_reward,
            total_cost,
            block_history: fake_block_history,
            _da_recording_cost_history: FakeDARecordingCostHistory,
            algorithm,
        }
    }
}

#[ignore]
#[test]
fn dummy() {}
