use std::{cmp::max, num::NonZeroU64};

use crate::utils::cumulative_percentage_change;

#[cfg(test)]
mod tests;

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum Error {
    #[error("Skipped L2 block update: expected {expected:?}, got {got:?}")]
    SkippedL2Block { expected: u32, got: u32 },
}

#[derive(Debug, Clone, PartialEq)]
pub struct AlgorithmV0 {
    /// The gas price for to cover the execution of the next block
    new_exec_price: u64,
    /// The block height of the next L2 block
    for_height: u32,
    /// The change percentage per block
    percentage: u64,
}

impl AlgorithmV0 {
    pub fn calculate(&self) -> u64 {
        self.new_exec_price
    }

    pub fn worst_case(&self, height: u32) -> u64 {
        cumulative_percentage_change(
            self.new_exec_price,
            self.for_height,
            self.percentage,
            height,
        )
    }
}

/// The state of the algorithm used to update the gas price algorithm for each block
///
/// Because there will always be a delay between blocks submitted to the L2 chain and the blocks
/// being recorded on the DA chain, the updater needs to make "projections" about the cost of
/// recording any given block to the DA chain. This is done by tracking the cost per byte of recording
/// for the most recent blocks, and using the known bytes of the unrecorded blocks to estimate
/// the cost for that block. Every time the DA recording is updated, the projections are recalculated.
///
/// This projection will inevitably lead to error in the gas price calculation. Special care should be taken
/// to account for the worst case scenario when calculating the parameters of the algorithm.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub struct AlgorithmUpdaterV0 {
    /// The gas price to cover the execution of the next block
    pub new_exec_price: u64,
    // Execution
    /// The lowest the algorithm allows the exec gas price to go
    pub min_exec_gas_price: u64,
    /// The Percentage the execution gas price will change in a single block, either increase or decrease
    /// based on the fullness of the last L2 block
    pub exec_gas_price_change_percent: u64,
    /// The height of the next L2 block
    pub l2_block_height: u32,
    /// The threshold of gas usage above and below which the gas price will increase or decrease
    /// This is a percentage of the total capacity of the L2 block
    pub l2_block_fullness_threshold_percent: u64,
}

impl AlgorithmUpdaterV0 {
    pub fn new(
        new_exec_price: u64,
        min_exec_gas_price: u64,
        exec_gas_price_change_percent: u64,
        l2_block_height: u32,
        l2_block_fullness_threshold_percent: u64,
    ) -> Self {
        let new_exec_price_corrected = max(new_exec_price, min_exec_gas_price);
        Self {
            new_exec_price: new_exec_price_corrected,
            min_exec_gas_price,
            exec_gas_price_change_percent,
            l2_block_height,
            l2_block_fullness_threshold_percent,
        }
    }

    pub fn update_l2_block_data(
        &mut self,
        height: u32,
        used: u64,
        capacity: NonZeroU64,
    ) -> Result<(), Error> {
        let expected = self.l2_block_height.saturating_add(1);
        if height != expected {
            Err(Error::SkippedL2Block {
                expected,
                got: height,
            })
        } else {
            self.l2_block_height = height;
            self.update_exec_gas_price(used, capacity);
            Ok(())
        }
    }

    fn update_exec_gas_price(&mut self, used: u64, capacity: NonZeroU64) {
        let mut exec_gas_price = self.new_exec_price;
        let fullness_percent = used
            .saturating_mul(100)
            .checked_div(capacity.into())
            .unwrap_or(self.l2_block_fullness_threshold_percent);

        match fullness_percent.cmp(&self.l2_block_fullness_threshold_percent) {
            std::cmp::Ordering::Greater | std::cmp::Ordering::Equal => {
                let change_amount = self.change_amount(exec_gas_price);
                exec_gas_price = exec_gas_price.saturating_add(change_amount);
            }
            std::cmp::Ordering::Less => {
                let change_amount = self.change_amount(exec_gas_price);
                exec_gas_price = exec_gas_price.saturating_sub(change_amount);
            }
        }
        self.new_exec_price = max(self.min_exec_gas_price, exec_gas_price);
    }

    fn change_amount(&self, principle: u64) -> u64 {
        principle
            .saturating_mul(self.exec_gas_price_change_percent)
            .saturating_div(100)
    }

    pub fn algorithm(&self) -> AlgorithmV0 {
        AlgorithmV0 {
            new_exec_price: self.new_exec_price,
            for_height: self.l2_block_height,
            percentage: self.exec_gas_price_change_percent,
        }
    }
}
