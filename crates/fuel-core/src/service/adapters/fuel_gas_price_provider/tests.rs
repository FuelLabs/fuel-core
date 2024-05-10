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

struct FakeProducerProfitIndex {
    total_as_of_block: TotalAsOfBlock,
}

impl ProducerProfitIndex for FakeProducerProfitIndex {
    fn total(&self) -> TotalAsOfBlock {
        self.total_as_of_block
    }

    fn update_profit(
        &mut self,
        _block_height: BlockHeight,
        _reward: u64,
        _cost: u64,
    ) -> ports::Result<()> {
        todo!()
    }
}

pub struct SimpleGasPriceAlgorithm;

impl GasPriceAlgorithm for SimpleGasPriceAlgorithm {
    fn calculate_gas_price(
        &self,
        previous_gas_price: u64,
        total_production_reward: u64,
        total_da_recording_cost: u64,
        _block_fullness: BlockFullness,
    ) -> u64 {
        if total_production_reward < total_da_recording_cost {
            previous_gas_price + 1
        } else {
            previous_gas_price
        }
    }
}

struct ProviderBuilder {
    latest_height: BlockHeight,
    historical_gas_price: HashMap<BlockHeight, u64>,
    total_as_of_block: TotalAsOfBlock,
}

impl ProviderBuilder {
    fn new() -> Self {
        let profit = TotalAsOfBlock {
            block_height: 0.into(),
            reward: 0,
            cost: 0,
        };
        Self {
            latest_height: 0.into(),
            historical_gas_price: HashMap::new(),
            total_as_of_block: profit,
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
        self.total_as_of_block = TotalAsOfBlock {
            block_height,
            reward,
            cost,
        };
        self
    }

    fn build_with_simple_algo(
        self,
    ) -> FuelGasPriceProvider<
        FakeBlockHistory,
        FakeDARecordingCostHistory,
        FakeProducerProfitIndex,
        SimpleGasPriceAlgorithm,
    > {
        let Self {
            latest_height,
            historical_gas_price,
            total_as_of_block,
        } = self;

        let fake_block_history = FakeBlockHistory {
            latest_height,
            history: historical_gas_price,
        };
        let fake_producer_profit_index = FakeProducerProfitIndex { total_as_of_block };
        FuelGasPriceProvider::new(
            fake_block_history,
            FakeDARecordingCostHistory,
            fake_producer_profit_index,
            SimpleGasPriceAlgorithm,
        )
    }
}

#[ignore]
#[test]
fn dummy() {}
