use crate::service::adapters::fuel_gas_price_provider::ports::{
    GasPriceAlgorithm,
    ProducerProfitIndex,
    TotalAsOfBlock,
};
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
pub struct FuelGasPriceProvider<FB, DA, PI, A> {
    block_history: FB,
    _da_recording_cost_history: DA,
    profit_index: PI,
    algorithm: A,
}

impl<FB, DA, PI, A> FuelGasPriceProvider<FB, DA, PI, A> {
    pub fn new(
        block_history: FB,
        da_recording_cost_history: DA,
        profit_index: PI,
        algorithm: A,
    ) -> Self {
        Self {
            block_history,
            _da_recording_cost_history: da_recording_cost_history,
            profit_index,
            algorithm,
        }
    }
}

impl<FB, DA, PI, A> FuelGasPriceProvider<FB, DA, PI, A>
where
    FB: FuelBlockHistory,
    DA: DARecordingCostHistory,
    PI: ProducerProfitIndex,
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
        let TotalAsOfBlock {
            block_height,
            reward,
            cost,
        } = self.profit_index.total();
        if block_height == self.block_history.latest_height() {
            let previous_gas_price = self.block_history.gas_price(block_height)?;
            let block_fullness = self.block_history.block_fullness(block_height)?;
            let new_gas_price = self.algorithm.calculate_gas_price(
                previous_gas_price,
                reward,
                cost,
                block_fullness,
            );
            Some(new_gas_price)
        } else {
            todo!("We should update the profit value with latest values")
        }
    }
}

// fn increase_gas_price(gas_price: u64) -> u64 {
//     // TODO: Choose meaningful increase based on inputs
//     gas_price + 1
// }

impl<FB, DA, PI, A> GasPriceProvider for FuelGasPriceProvider<FB, DA, PI, A>
where
    FB: FuelBlockHistory,
    DA: DARecordingCostHistory,
    PI: ProducerProfitIndex,
    A: GasPriceAlgorithm,
{
    fn gas_price(&self, params: GasPriceParams) -> Option<u64> {
        self.inner_gas_price(params.block_height())
    }
}
