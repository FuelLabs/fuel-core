#![allow(non_snake_case)]

use super::*;
use crate::service::adapters::{
    fuel_gas_price_provider::ports::{
        BlockProductionReward,
        DARecordingCost,
        FuelBlockGasPriceInputs,
        ProfitAsOfBlock,
    },
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

    fn gas_price_inputs(&self, height: BlockHeight) -> Option<FuelBlockGasPriceInputs> {
        self.history
            .get(&height)
            .map(|&gas_price| FuelBlockGasPriceInputs::new(gas_price))
    }
}

struct FakeDARecordingCostHistory;

impl DARecordingCostHistory for FakeDARecordingCostHistory {
    fn recording_cost(&self, _height: BlockHeight) -> Option<DARecordingCost> {
        todo!();
    }
}

struct FakeProducerProfitIndex {
    profit: ProfitAsOfBlock,
}

impl ProducerProfitIndex for FakeProducerProfitIndex {
    fn profit(&self) -> ProfitAsOfBlock {
        self.profit
    }

    fn update_profit(
        &mut self,
        _block_height: BlockHeight,
        _reward: BlockProductionReward,
        _cost: DARecordingCost,
    ) -> ports::Result<()> {
        todo!()
    }
}

struct ProviderBuilder {
    latest_height: BlockHeight,
    historical_gas_price: HashMap<BlockHeight, u64>,
    profit: ProfitAsOfBlock,
}

impl ProviderBuilder {
    fn new() -> Self {
        let profit = ProfitAsOfBlock {
            profit: 0,
            block_height: 0.into(),
        };
        Self {
            latest_height: 0.into(),
            historical_gas_price: HashMap::new(),
            profit,
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

    fn with_profit(mut self, profit: i64, block_height: BlockHeight) -> Self {
        self.profit = ProfitAsOfBlock {
            profit,
            block_height,
        };
        self
    }

    fn build(
        self,
    ) -> FuelGasPriceProvider<
        FakeBlockHistory,
        FakeDARecordingCostHistory,
        FakeProducerProfitIndex,
    > {
        let Self {
            latest_height,
            historical_gas_price,
            profit,
        } = self;

        let fake_block_history = FakeBlockHistory {
            latest_height,
            history: historical_gas_price,
        };
        let fake_producer_profit_index = FakeProducerProfitIndex { profit };
        FuelGasPriceProvider::new(
            fake_block_history,
            FakeDARecordingCostHistory,
            fake_producer_profit_index,
        )
    }
}

#[ignore]
#[test]
fn dummy() {}
