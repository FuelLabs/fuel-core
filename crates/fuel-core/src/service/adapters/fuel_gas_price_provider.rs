use crate::service::adapters::fuel_gas_price_provider::ports::{
    ProducerProfitIndex,
    ProfitAsOfBlock,
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
pub struct FuelGasPriceProvider<FB, DA, PI> {
    block_history: FB,
    _da_recording_cost_history: DA,
    profit_index: PI,
}

impl<FB, DA, PI> FuelGasPriceProvider<FB, DA, PI> {
    pub fn new(
        block_history: FB,
        da_recording_cost_history: DA,
        profit_index: PI,
    ) -> Self {
        Self {
            block_history,
            _da_recording_cost_history: da_recording_cost_history,
            profit_index,
        }
    }
}

impl<FB, DA, PI> FuelGasPriceProvider<FB, DA, PI>
where
    FB: FuelBlockHistory,
    DA: DARecordingCostHistory,
    PI: ProducerProfitIndex,
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
        let ProfitAsOfBlock {
            profit,
            block_height,
        } = self.profit_index.profit();
        if block_height == self.block_history.latest_height() {
            let gas_price = self.block_history.gas_price(block_height)?;
            if profit < 0 {
                let new_gas_price = increase_gas_price(gas_price);
                Some(new_gas_price)
            } else {
                Some(gas_price)
            }
        } else {
            todo!("We should update the profit value with latest values")
        }
    }
}

fn increase_gas_price(gas_price: u64) -> u64 {
    // TODO: Choose meaningful increase based on inputs
    gas_price + 1
}

impl<FB, DA, PI> GasPriceProvider for FuelGasPriceProvider<FB, DA, PI>
where
    FB: FuelBlockHistory,
    DA: DARecordingCostHistory,
    PI: ProducerProfitIndex,
{
    fn gas_price(&self, params: GasPriceParams) -> Option<u64> {
        self.inner_gas_price(params.block_height())
    }
}
