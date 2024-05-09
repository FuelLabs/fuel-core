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
pub struct FuelGasPriceProvider<FB, DA> {
    block_history: FB,
    _da_recording_cost_history: DA,
}

impl<FB, DA> FuelGasPriceProvider<FB, DA> {
    pub fn new(block_history: FB, da_recording_cost_history: DA) -> Self {
        Self {
            block_history,
            _da_recording_cost_history: da_recording_cost_history,
        }
    }
}

impl<FB, DA> FuelGasPriceProvider<FB, DA>
where
    FB: FuelBlockHistory,
    DA: DARecordingCostHistory,
{
    fn inner_gas_price(&self, block_height: BlockHeight) -> Option<u64> {
        let latest_block = self.block_history.latest_height();
        if latest_block > block_height {
            self.block_history.gas_price(block_height)
        } else if *latest_block + 1 == *block_height {
            let arbitrary_cost = 237894;
            Some(arbitrary_cost)
        } else {
            // TODO: Should we return an error instead?
            None
        }
    }
}

impl<FB, DA> GasPriceProvider for FuelGasPriceProvider<FB, DA>
where
    FB: FuelBlockHistory,
    DA: DARecordingCostHistory,
{
    fn gas_price(&self, params: GasPriceParams) -> Option<u64> {
        self.inner_gas_price(params.block_height())
    }
}
