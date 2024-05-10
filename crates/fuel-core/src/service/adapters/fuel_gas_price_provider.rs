use crate::service::adapters::fuel_gas_price_provider::ports::GasPriceAlgorithm;
use fuel_core_producer::block_producer::gas_price::{
    GasPriceParams,
    GasPriceProvider,
};
use fuel_core_types::fuel_types::BlockHeight;
use ports::{
    DARecordingCostHistory,
    FuelBlockHistory,
};

pub mod ports;

#[cfg(test)]
mod tests;

/// Gives the gas price for a given block height, and calculates the gas price if not yet committed.
pub struct FuelGasPriceProvider<FB, DA, A> {
    // totals
    totaled_block_height: BlockHeight,
    total_reward: u64,
    total_cost: u64,

    // adapters
    block_history: FB,
    _da_recording_cost_history: DA,
    algorithm: A,
}

impl<FB, DA, A> FuelGasPriceProvider<FB, DA, A> {
    pub fn new(block_history: FB, da_recording_cost_history: DA, algorithm: A) -> Self {
        Self {
            totaled_block_height: 0.into(),
            total_reward: 0,
            total_cost: 0,
            block_history,
            _da_recording_cost_history: da_recording_cost_history,
            algorithm,
        }
    }
}

impl<FB, DA, A> FuelGasPriceProvider<FB, DA, A>
where
    FB: FuelBlockHistory,
    DA: DARecordingCostHistory,
    A: GasPriceAlgorithm,
{
    fn inner_gas_price(&self, requested_block_height: BlockHeight) -> Option<u64> {
        let latest_block = self.block_history.latest_height();
        if latest_block > requested_block_height {
            self.block_history.gas_price(requested_block_height)
        } else if Self::asking_for_next_block(latest_block, requested_block_height) {
            self.calculate_new_gas_price()
        } else {
            // TODO: Should we return an error instead?
            None
        }
    }

    fn asking_for_next_block(
        latest_block: BlockHeight,
        block_height: BlockHeight,
    ) -> bool {
        *latest_block + 1 == *block_height
    }

    fn calculate_new_gas_price(&self) -> Option<u64> {
        let latest_gas_price = self.block_history.latest_height();
        if self.totaled_block_height == latest_gas_price {
            let previous_gas_price = self.block_history.gas_price(latest_gas_price)?;
            let block_fullness = self.block_history.block_fullness(latest_gas_price)?;
            let new_gas_price_candidate = self.algorithm.calculate_gas_price(
                previous_gas_price,
                self.total_reward,
                self.total_cost,
                block_fullness,
            );
            let new_gas_price = core::cmp::min(
                new_gas_price_candidate,
                self.algorithm.maximum_next_gas_price(previous_gas_price),
            );
            Some(new_gas_price)
        } else {
            todo!("We should update the profit value with latest values")
        }
    }
}

impl<FB, DA, A> GasPriceProvider for FuelGasPriceProvider<FB, DA, A>
where
    FB: FuelBlockHistory,
    DA: DARecordingCostHistory,
    A: GasPriceAlgorithm,
{
    fn gas_price(&self, params: GasPriceParams) -> Option<u64> {
        self.inner_gas_price(params.block_height())
    }
}
