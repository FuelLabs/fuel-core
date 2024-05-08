#![allow(non_snake_case)]

use super::*;
use crate::service::adapters::fuel_gas_price_provider::ports::{
    BlockFullness,
    BlockProductionReward,
    DARecordingCost,
};
use std::collections::HashMap;

struct FakeBlockHistory;

impl BlockHistory for FakeBlockHistory {
    fn latest_height(&self) -> BlockHeight {
        todo!()
    }
}

struct FakeGasPriceHistory {
    history: HashMap<BlockHeight, GasPrice>,
}

impl GasPriceHistory for FakeGasPriceHistory {
    fn gas_price(&self, height: BlockHeight) -> Option<GasPrice> {
        self.history.get(&height).copied()
    }
}

struct FakeBlockFullnessHistory;

impl BlockFullnessHistory for FakeBlockFullnessHistory {
    fn block_fullness(&self, _height: BlockHeight) -> BlockFullness {
        todo!();
    }
}

struct FakeBlockProductionRewardHistory;

impl FuelBlockProductionRewardHistory for FakeBlockProductionRewardHistory {
    fn block_production_reward(&self, _height: BlockHeight) -> BlockProductionReward {
        todo!();
    }
}

struct FakeDARecordingCostHistory;

impl DARecordingCostHistory for FakeDARecordingCostHistory {
    fn recording_cost(&self, _height: BlockHeight) -> DARecordingCost {
        todo!();
    }
}

struct ProviderBuilder {
    historical_gas_price: HashMap<BlockHeight, GasPrice>,
}

impl ProviderBuilder {
    fn new() -> Self {
        Self {
            historical_gas_price: HashMap::new(),
        }
    }

    fn with_historical_gas_price(
        mut self,
        block_height: BlockHeight,
        gas_price: GasPrice,
    ) -> Self {
        self.historical_gas_price.insert(block_height, gas_price);
        self
    }

    fn build(
        self,
    ) -> FuelGasPriceProvider<
        FakeBlockHistory,
        FakeGasPriceHistory,
        FakeBlockFullnessHistory,
        FakeBlockProductionRewardHistory,
        FakeDARecordingCostHistory,
    > {
        let gas_price_history = FakeGasPriceHistory {
            history: self.historical_gas_price,
        };
        FuelGasPriceProvider::new(
            FakeBlockHistory,
            gas_price_history,
            FakeBlockFullnessHistory,
            FakeBlockProductionRewardHistory,
            FakeDARecordingCostHistory,
        )
    }
}

#[test]
fn gas_price__can_get_a_historical_gas_price() {
    // given
    let block_height = 432.into();
    let expected = 123;
    let gas_price_provider = ProviderBuilder::new()
        .with_historical_gas_price(block_height, expected)
        .build();

    // when
    let actual = gas_price_provider.gas_price(block_height).unwrap();

    // then
    assert_eq!(actual, expected);
}
