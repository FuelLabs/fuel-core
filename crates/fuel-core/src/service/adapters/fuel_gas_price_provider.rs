use fuel_core_txpool::{
    ports::GasPriceProvider,
    types::GasPrice,
};
use fuel_core_types::fuel_types::BlockHeight;

pub struct BlockFullness;

pub struct DARecordingCost;

/// Gives the gas price for a given block height, and calculates the gas price if not yet committed.
pub struct FuelGasPriceProvider<B, GP, BF, DA> {
    _block_history: B,
    gas_price_history: GP,
    _block_fullness_history: BF,
    _da_recording_cost_history: DA,
}

impl<B, GP, BF, DA> FuelGasPriceProvider<B, GP, BF, DA> {
    pub fn new(
        block_history: B,
        gas_price_history: GP,
        block_fullness_history: BF,
        da_recording_cost_history: DA,
    ) -> Self {
        Self {
            _block_history: block_history,
            gas_price_history,
            _block_fullness_history: block_fullness_history,
            _da_recording_cost_history: da_recording_cost_history,
        }
    }
}

impl<B, GP, BF, DA> GasPriceProvider for FuelGasPriceProvider<B, GP, BF, DA>
where
    B: BlockHistory,
    GP: GasPriceHistory,
    BF: BlockFullnessHistory,
    DA: DARecordingCostHistory,
{
    fn gas_price(&self, height: BlockHeight) -> Option<GasPrice> {
        self.gas_price_history.gas_price(height)
    }
}

pub trait BlockHistory {
    fn latest_height(&self) -> BlockHeight;
}

pub trait GasPriceHistory {
    fn gas_price(&self, height: BlockHeight) -> Option<GasPrice>;
}

pub trait BlockFullnessHistory {
    fn block_fullness(&self, height: BlockHeight) -> BlockFullness;
}

pub trait DARecordingCostHistory {
    fn recording_cost(&self, height: BlockHeight) -> DARecordingCost;
}

#[allow(non_snake_case)]
#[cfg(test)]
mod tests {
    use super::*;
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
            FakeDARecordingCostHistory,
        > {
            let gas_price_history = FakeGasPriceHistory {
                history: self.historical_gas_price,
            };
            FuelGasPriceProvider::new(
                FakeBlockHistory,
                gas_price_history,
                FakeBlockFullnessHistory,
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
}
