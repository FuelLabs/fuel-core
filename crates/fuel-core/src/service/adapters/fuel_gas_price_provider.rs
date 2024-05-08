use fuel_core_txpool::{
    ports::GasPriceProvider,
    types::GasPrice,
};
use fuel_core_types::fuel_types::BlockHeight;

pub struct BlockFullness;

pub struct DARecordingCost;

/// Calculates
pub struct FuelGasPriceProvider;

impl GasPriceProvider for FuelGasPriceProvider {
    fn gas_price(&self, _height: BlockHeight) -> Option<GasPrice> {
        let arbitrary_gas_price = 999;
        Some(arbitrary_gas_price)
    }
}

pub trait BlockHistory {
    fn latest_height(&self) -> BlockHeight;
}

pub trait GasPriceHistory {
    fn gas_price(&self, height: BlockHeight) -> GasPrice;
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

    struct FakeBlockHistory;

    impl BlockHistory for FakeBlockHistory {
        fn latest_height(&self) -> BlockHeight {
            todo!()
        }
    }

    struct FakeGasPriceHistory;

    impl GasPriceHistory for FakeGasPriceHistory {
        fn gas_price(&self, _height: BlockHeight) -> GasPrice {
            todo!();
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

    #[test]
    fn gas_price__can_get_a_gas_price() {
        // given
        let gas_price_provider = FuelGasPriceProvider;

        // when
        let block_height = 0.into();
        let gas_price = gas_price_provider.gas_price(block_height);

        // then
        assert!(gas_price.is_some());
    }
}
