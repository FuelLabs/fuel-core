use crate::service::adapters::fuel_gas_price_provider::ports::{
    GasPriceAlgorithm,
    GasPriceHistory,
    GasPrices,
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
use std::cell::RefCell;

pub mod ports;

pub mod algorithm_adapter;

use ports::{
    Error,
    Result,
};

#[cfg(test)]
mod tests;

/// Gives the gas price for a given block height, and calculates the gas price if not yet committed.
pub struct FuelGasPriceProvider<FB, DA, A, GP> {
    profitablility_totals: ProfitablilityTotals,

    // adapters
    block_history: FB,
    da_recording_cost_history: DA,
    algorithm: A,
    gas_price_history: GP,
}

impl<FB, DA, A, GP> FuelGasPriceProvider<FB, DA, A, GP> {
    pub fn new(
        block_history: FB,
        da_recording_cost_history: DA,
        algorithm: A,
        gas_price_history: GP,
    ) -> Self {
        Self {
            profitablility_totals: ProfitablilityTotals::default(),
            block_history,
            da_recording_cost_history,
            algorithm,
            gas_price_history,
        }
    }
}

struct ProfitablilityTotals {
    totaled_block_height: RefCell<BlockHeight>,
    total_reward: RefCell<u64>,
    total_cost: RefCell<u64>,
}

impl Default for ProfitablilityTotals {
    fn default() -> Self {
        Self {
            totaled_block_height: RefCell::new(0.into()),
            total_reward: RefCell::new(0),
            total_cost: RefCell::new(0),
        }
    }
}

impl ProfitablilityTotals {
    #[allow(dead_code)]
    pub fn new(block_height: BlockHeight, reward: u64, cost: u64) -> Self {
        Self {
            totaled_block_height: RefCell::new(block_height),
            total_reward: RefCell::new(reward),
            total_cost: RefCell::new(cost),
        }
    }

    fn update(&self, block_height: BlockHeight, reward: u64, cost: u64) {
        let mut totaled_block_height = self.totaled_block_height.borrow_mut();
        let mut total_reward = self.total_reward.borrow_mut();
        let mut total_cost = self.total_cost.borrow_mut();
        *totaled_block_height = block_height;
        *total_reward += reward;
        *total_cost += cost;
    }
}

impl<FB, DA, A, GP> FuelGasPriceProvider<FB, DA, A, GP>
where
    FB: FuelBlockHistory,
    DA: DARecordingCostHistory,
    A: GasPriceAlgorithm,
    GP: GasPriceHistory,
{
    fn inner_gas_price(&self, requested_block_height: BlockHeight) -> Result<u64> {
        let latest_block = self
            .block_history
            .latest_height()
            .map_err(Error::UnableToGetLatestBlockHeight)?;
        if latest_block > requested_block_height {
            let gas_prices = self
                .gas_price_history
                .gas_prices(requested_block_height)
                .map_err(Error::UnableToGetGasPrices)?
                .ok_or(Error::GasPricesNotFoundForBlockHeight(
                    requested_block_height,
                ))?;
            Ok(gas_prices.total())
        } else if Self::asking_for_next_block(latest_block, requested_block_height) {
            let new_gas_prices = self.calculate_new_gas_price(latest_block)?;
            self.gas_price_history
                .store_gas_prices(latest_block, &new_gas_prices)
                .map_err(Error::UnableToStoreGasPrices)?;
            Ok(new_gas_prices.total())
        } else {
            Err(Error::RequestedBlockHeightTooHigh {
                requested: requested_block_height,
                latest: latest_block,
            })
        }
    }

    fn asking_for_next_block(
        latest_block: BlockHeight,
        block_height: BlockHeight,
    ) -> bool {
        *latest_block + 1 == *block_height
    }

    fn calculate_new_gas_price(
        &self,
        latest_block_height: BlockHeight,
    ) -> Result<GasPrices> {
        self.update_totals(latest_block_height)?;
        let previous_gas_prices = self
            .gas_price_history
            .gas_prices(latest_block_height)
            .map_err(Error::UnableToGetGasPrices)?
            .ok_or(Error::GasPricesNotFoundForBlockHeight(latest_block_height))?;
        let block_fullness = self
            .block_history
            .block_fullness(latest_block_height)
            .map_err(Error::UnableToGetBlockFullness)?
            .ok_or(Error::BlockFullnessNotFoundForBlockHeight(
                latest_block_height,
            ))?;
        let new_gas_prices = self.algorithm.calculate_gas_prices(
            previous_gas_prices,
            self.total_reward(),
            self.total_cost(),
            block_fullness,
        );

        Ok(new_gas_prices)
    }

    fn total_reward(&self) -> u64 {
        *self.profitablility_totals.total_reward.borrow()
    }

    fn total_cost(&self) -> u64 {
        *self.profitablility_totals.total_cost.borrow()
    }

    fn totaled_block_height(&self) -> BlockHeight {
        *self.profitablility_totals.totaled_block_height.borrow()
    }

    fn update_totals(&self, latest_block_height: BlockHeight) -> Result<()> {
        while self.totaled_block_height() < latest_block_height {
            let block_height = (*self.totaled_block_height() + 1).into();
            let reward = self
                .block_history
                .production_reward(block_height)
                .map_err(Error::UnableToGetProductionReward)?
                .ok_or(Error::ProductionRewardNotFoundForBlockHeight(block_height))?;
            let cost = self
                .da_recording_cost_history
                .recording_cost(block_height)
                .map_err(Error::UnableToGetRecordingCost)?
                .ok_or(Error::RecordingCostNotFoundForBlockHeight(block_height))?;
            self.profitablility_totals
                .update(block_height, reward, cost);
        }
        Ok(())
    }
}

impl<FB, DA, A, GP> GasPriceProvider for FuelGasPriceProvider<FB, DA, A, GP>
where
    FB: FuelBlockHistory,
    DA: DARecordingCostHistory,
    A: GasPriceAlgorithm,
    GP: GasPriceHistory,
{
    fn gas_price(
        &self,
        params: GasPriceParams,
    ) -> Result<u64, Box<dyn std::error::Error>> {
        self.inner_gas_price(params.block_height())
            .map_err(|e| Box::new(e).into())
    }
}
