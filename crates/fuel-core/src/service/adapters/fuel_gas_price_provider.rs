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
use std::cell::RefCell;

pub mod ports;

use ports::{
    Error,
    Result,
};

#[cfg(test)]
mod tests;

/// Gives the gas price for a given block height, and calculates the gas price if not yet committed.
pub struct FuelGasPriceProvider<FB, DA, A> {
    profitablility_totals: ProfitablilityTotals,

    // adapters
    block_history: FB,
    da_recording_cost_history: DA,
    algorithm: A,
}

impl<FB, DA, A> FuelGasPriceProvider<FB, DA, A> {
    pub fn new(block_history: FB, da_recording_cost_history: DA, algorithm: A) -> Self {
        Self {
            profitablility_totals: ProfitablilityTotals::default(),
            block_history,
            da_recording_cost_history,
            algorithm,
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
        while *totaled_block_height < block_height {
            *totaled_block_height = block_height;
            *total_reward += reward;
            *total_cost += cost;
        }
    }
}

impl<FB, DA, A> FuelGasPriceProvider<FB, DA, A>
where
    FB: FuelBlockHistory,
    DA: DARecordingCostHistory,
    A: GasPriceAlgorithm,
{
    fn inner_gas_price(&self, requested_block_height: BlockHeight) -> Result<u64> {
        let latest_block = self
            .block_history
            .latest_height()
            .map_err(Error::UnableToGetLatestBlockHeight)?;
        if latest_block > requested_block_height {
            self.block_history
                .gas_price(requested_block_height)
                .map_err(Error::UnableToGetGasPrice)?
                .ok_or(Error::GasPriceNotFoundForBlockHeight(
                    requested_block_height,
                ))
        } else if Self::asking_for_next_block(latest_block, requested_block_height) {
            self.calculate_new_gas_price(latest_block)
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

    fn calculate_new_gas_price(&self, latest_block_height: BlockHeight) -> Result<u64> {
        self.update_totals(latest_block_height)?;
        let previous_gas_price = self
            .block_history
            .gas_price(latest_block_height)
            .map_err(Error::UnableToGetGasPrice)?
            .ok_or(Error::GasPriceNotFoundForBlockHeight(latest_block_height))?;
        let block_fullness = self
            .block_history
            .block_fullness(latest_block_height)
            .map_err(Error::UnableToGetBlockFullness)?
            .ok_or(Error::BlockFullnessNotFoundForBlockHeight(
                latest_block_height,
            ))?;
        let new_gas_price_candidate = self.algorithm.calculate_gas_price(
            previous_gas_price,
            self.total_reward(),
            self.total_cost(),
            block_fullness,
        );
        let new_gas_price = core::cmp::min(
            new_gas_price_candidate,
            self.algorithm.maximum_next_gas_price(previous_gas_price),
        );
        Ok(new_gas_price)
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

impl<FB, DA, A> GasPriceProvider for FuelGasPriceProvider<FB, DA, A>
where
    FB: FuelBlockHistory,
    DA: DARecordingCostHistory,
    A: GasPriceAlgorithm,
{
    fn gas_price(&self, params: GasPriceParams) -> Option<u64> {
        // TODO: handle error
        self.inner_gas_price(params.block_height()).ok()
    }
}
