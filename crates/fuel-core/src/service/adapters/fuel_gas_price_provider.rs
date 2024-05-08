use fuel_core_producer::block_producer::gas_price::{
    GasPriceParams,
    GasPriceProvider,
};
use fuel_core_types::fuel_types::BlockHeight;
use ports::{
    BlockFullnessHistory,
    BlockHistory,
    DARecordingCostHistory,
    FuelBlockProductionRewardHistory,
    GasPriceHistory,
};

pub mod ports;

#[cfg(test)]
mod tests;

/// Gives the gas price for a given block height, and calculates the gas price if not yet committed.
pub struct FuelGasPriceProvider<B, GP, BF, BP, DA> {
    _block_history: B,
    gas_price_history: GP,
    _block_fullness_history: BF,
    _block_production_reward: BP,
    _da_recording_cost_history: DA,
}

impl<B, GP, BF, BP, DA> FuelGasPriceProvider<B, GP, BF, BP, DA> {
    pub fn new(
        block_history: B,
        gas_price_history: GP,
        block_fullness_history: BF,
        block_production_reward: BP,
        da_recording_cost_history: DA,
    ) -> Self {
        Self {
            _block_history: block_history,
            gas_price_history,
            _block_fullness_history: block_fullness_history,
            _block_production_reward: block_production_reward,
            _da_recording_cost_history: da_recording_cost_history,
        }
    }
}

impl<B, GP, BF, BP, DA> FuelGasPriceProvider<B, GP, BF, BP, DA>
where
    B: BlockHistory,
    GP: GasPriceHistory,
    BF: BlockFullnessHistory,
    BP: FuelBlockProductionRewardHistory,
    DA: DARecordingCostHistory,
{
    fn inner_gas_price(&self, block_height: BlockHeight) -> Option<u64> {
        let latest_block = self._block_history.latest_height();
        if latest_block > block_height {
            self.gas_price_history.gas_price(block_height)
        } else if *latest_block + 1 == *block_height {
            let arbitrary_cost = 237894;
            Some(arbitrary_cost)
        } else {
            // TODO: Should we return an error instead?
            None
        }
    }
}

impl<B, GP, BF, BP, DA> GasPriceProvider for FuelGasPriceProvider<B, GP, BF, BP, DA>
where
    B: BlockHistory,
    GP: GasPriceHistory,
    BF: BlockFullnessHistory,
    BP: FuelBlockProductionRewardHistory,
    DA: DARecordingCostHistory,
{
    fn gas_price(&self, params: GasPriceParams) -> Option<u64> {
        self.inner_gas_price(params.block_height())
    }
}
