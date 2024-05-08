use fuel_core_txpool::{
    ports::GasPriceProvider,
    types::GasPrice,
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

impl<B, GP, BF, BP, DA> GasPriceProvider for FuelGasPriceProvider<B, GP, BF, BP, DA>
where
    B: BlockHistory,
    GP: GasPriceHistory,
    BF: BlockFullnessHistory,
    BP: FuelBlockProductionRewardHistory,
    DA: DARecordingCostHistory,
{
    fn gas_price(&self, height: BlockHeight) -> Option<GasPrice> {
        self.gas_price_history.gas_price(height)
    }
}
