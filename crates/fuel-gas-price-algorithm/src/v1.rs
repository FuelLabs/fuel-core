use std::{
    cmp::max,
    num::NonZeroU64,
    ops::Div,
};

use crate::utils::cumulative_percentage_change;

#[cfg(test)]
mod tests;

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum Error {
    #[error("Skipped L2 block update: expected {expected:?}, got {got:?}")]
    SkippedL2Block { expected: u32, got: u32 },
    #[error("Skipped DA block update: expected {expected:?}, got {got:?}")]
    SkippedDABlock { expected: u32, got: u32 },
    #[error("Could not calculate cost per byte: {bytes:?} bytes, {cost:?} cost")]
    CouldNotCalculateCostPerByte { bytes: u64, cost: u64 },
    #[error("Failed to include L2 block data: {0}")]
    FailedTooIncludeL2BlockData(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct AlgorithmV1 {
    /// The gas price for to cover the execution of the next block
    new_exec_price: u64,
    /// The change percentage per block
    exec_price_percentage: u64,
    /// The gas price for to cover DA commitment
    new_da_gas_price: u64,
    /// The change percentage per block
    da_gas_price_percentage: u64,
    /// The block height of the next L2 block
    for_height: u32,
}

impl AlgorithmV1 {
    pub fn calculate(&self) -> u64 {
        self.new_exec_price.saturating_add(self.new_da_gas_price)
    }

    pub fn worst_case(&self, height: u32) -> u64 {
        let exec = cumulative_percentage_change(
            self.new_exec_price,
            self.for_height,
            self.exec_price_percentage,
            height,
        );
        let da = cumulative_percentage_change(
            self.new_da_gas_price,
            self.for_height,
            self.da_gas_price_percentage,
            height,
        );
        exec.saturating_add(da)
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
///
/// An algorithm for calculating the gas price for the next block
///
/// The algorithm breaks up the gas price into two components:
/// - The execution gas price, which is used to cover the cost of executing the next block as well
///   as moderating the congestion of the network by increasing the price when traffic is high.
/// - The data availability (DA) gas price, which is used to cover the cost of recording the block on the DA chain
///
/// The execution gas price is calculated based on the fullness of the last received l2 block. Each
/// block has a capacity threshold, and if the block is above this threshold, the gas price is increased. If
/// it is below the threshold, the gas price is decreased.
/// The gas price can only change by a fixed amount each block.
///
/// The DA gas price is calculated based on the profit of previous blocks. The profit is the
/// difference between the rewards from the DA portion of the gas price and the cost of recording the blocks on the DA chain.
/// The algorithm uses a naive PID controller to calculate the change in the DA gas price. The "P" portion
/// of the new gas price is "proportional" to the profit, either negative or positive. The "D" portion is derived
/// from the slope or change in the profits since the last block.
///
/// if p > 0 and dp/db > 0, decrease
/// if p > 0 and dp/db < 0, hold/moderate
/// if p < 0 and dp/db < 0, increase
/// if p < 0 and dp/db > 0, hold/moderate
///
/// The DA portion also uses a moving average of the profits over the last `avg_window` blocks
/// instead of the actual profit. Setting the `avg_window` to 1 will effectively disable the
/// moving average.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub struct AlgorithmUpdaterV1 {
    // Execution
    /// The gas price (scaled by the `gas_price_factor`) to cover the execution of the next block
    pub new_scaled_exec_price: u64,
    /// The lowest the algorithm allows the exec gas price to go
    pub min_exec_gas_price: u64,
    /// The Percentage the execution gas price will change in a single block, either increase or decrease
    /// based on the fullness of the last L2 block. Using `u16` because it can go above 100% and
    /// possibly over 255%
    pub exec_gas_price_change_percent: u16,
    /// The height of the next L2 block
    pub l2_block_height: u32,
    /// The threshold of gas usage above and below which the gas price will increase or decrease
    /// This is a percentage of the total capacity of the L2 block
    pub l2_block_fullness_threshold_percent: ClampedPercentage,
    // DA
    /// The gas price for the DA portion of the last block. This can be used to calculate
    /// the DA portion of the next block
    // pub last_da_gas_price: u64,

    /// The gas price (scaled by the `gas_price_factor`) to cover the DA commitment of the next block
    pub new_scaled_da_gas_price: u64,

    /// Scale factor for the gas price.
    pub gas_price_factor: NonZeroU64,
    /// The lowest the algorithm allows the da gas price to go
    pub min_da_gas_price: u64,
    /// The maximum percentage that the DA portion of the gas price can change in a single block
    ///   Using `u16` because it can go above 100% and possibly over 255%
    pub max_da_gas_price_change_percent: u16,
    /// The cumulative reward from the DA portion of the gas price
    pub total_da_rewards_excess: u128,
    /// The height of the last L2 block recorded on the DA chain
    pub da_recorded_block_height: u32,
    /// The cumulative cost of recording L2 blocks on the DA chain as of the last recorded block
    pub latest_known_total_da_cost_excess: u128,
    /// The predicted cost of recording L2 blocks on the DA chain as of the last L2 block
    /// (This value is added on top of the `latest_known_total_da_cost` if the L2 height is higher)
    pub projected_total_da_cost: u128,
    /// The P component of the PID control for the DA gas price
    pub da_p_component: i64,
    /// The D component of the PID control for the DA gas price
    pub da_d_component: i64,
    /// The last profit
    pub last_profit: i128,
    /// The profit before last
    pub second_to_last_profit: i128,
    /// The latest known cost per byte for recording blocks on the DA chain
    pub latest_da_cost_per_byte: u128,
    /// The unrecorded blocks that are used to calculate the projected cost of recording blocks
    pub unrecorded_blocks: Vec<BlockBytes>,
}

/// A value that represents a value between 0 and 100. Higher values are clamped to 100
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub struct ClampedPercentage {
    value: u8,
}

impl ClampedPercentage {
    pub fn new(maybe_value: u8) -> Self {
        Self {
            value: maybe_value.min(100),
        }
    }
}

impl From<u8> for ClampedPercentage {
    fn from(value: u8) -> Self {
        Self::new(value)
    }
}

impl core::ops::Deref for ClampedPercentage {
    type Target = u8;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

#[derive(Debug, Clone)]
pub struct RecordedBlock {
    pub height: u32,
    pub block_bytes: u64,
    pub block_cost: u64,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub struct BlockBytes {
    pub height: u32,
    pub block_bytes: u64,
}

impl AlgorithmUpdaterV1 {
    pub fn update_da_record_data(
        &mut self,
        blocks: &[RecordedBlock],
    ) -> Result<(), Error> {
        for block in blocks {
            self.da_block_update(block.height, block.block_bytes, block.block_cost)?;
        }
        self.recalculate_projected_cost();
        self.normalize_rewards_and_costs();
        Ok(())
    }

    pub fn update_l2_block_data(
        &mut self,
        height: u32,
        used: u64,
        capacity: NonZeroU64,
        block_bytes: u64,
        fee_wei: u64,
    ) -> Result<(), Error> {
        let expected = self.l2_block_height.saturating_add(1);
        if height != expected {
            Err(Error::SkippedL2Block {
                expected,
                got: height,
            })
        } else {
            self.l2_block_height = height;

            // rewards
            self.update_rewards(fee_wei);
            let rewards = self.clamped_rewards_as_i128();

            // costs
            self.update_projected_cost(block_bytes);
            let projected_total_da_cost = self.clamped_projected_cost_as_i128();
            let last_profit = rewards.saturating_sub(projected_total_da_cost);
            self.update_last_profit(last_profit);

            // gas prices
            self.update_exec_gas_price(used, capacity);
            self.update_da_gas_price();

            // metadata
            self.unrecorded_blocks.push(BlockBytes {
                height,
                block_bytes,
            });
            Ok(())
        }
    }

    fn update_rewards(&mut self, fee_wei: u64) {
        let block_da_reward = self.da_portion_of_fee(fee_wei);
        self.total_da_rewards_excess = self
            .total_da_rewards_excess
            .saturating_add(block_da_reward.into());
    }

    fn update_projected_cost(&mut self, block_bytes: u64) {
        let block_projected_da_cost =
            (block_bytes as u128).saturating_mul(self.latest_da_cost_per_byte);
        self.projected_total_da_cost = self
            .projected_total_da_cost
            .saturating_add(block_projected_da_cost);
    }

    // Take the `fee_wei` and return the portion of the fee that should be used for paying DA costs
    fn da_portion_of_fee(&self, fee_wei: u64) -> u64 {
        // fee_wei * (da_price / (exec_price + da_price))
        let numerator = fee_wei.saturating_mul(self.descaled_da_price());
        let denominator = self
            .descaled_exec_price()
            .saturating_add(self.descaled_da_price());
        if denominator == 0 {
            0
        } else {
            numerator.div_ceil(denominator)
        }
    }

    fn clamped_projected_cost_as_i128(&self) -> i128 {
        i128::try_from(self.projected_total_da_cost).unwrap_or(i128::MAX)
    }

    fn clamped_rewards_as_i128(&self) -> i128 {
        i128::try_from(self.total_da_rewards_excess).unwrap_or(i128::MAX)
    }

    fn update_last_profit(&mut self, new_profit: i128) {
        self.second_to_last_profit = self.last_profit;
        self.last_profit = new_profit;
    }

    fn update_exec_gas_price(&mut self, used: u64, capacity: NonZeroU64) {
        let threshold = *self.l2_block_fullness_threshold_percent as u64;
        let mut scaled_exec_gas_price = self.new_scaled_exec_price;
        let fullness_percent = used
            .saturating_mul(100)
            .checked_div(capacity.into())
            .unwrap_or(threshold);

        match fullness_percent.cmp(&threshold) {
            std::cmp::Ordering::Greater => {
                let change_amount = self.exec_change(scaled_exec_gas_price);
                scaled_exec_gas_price =
                    scaled_exec_gas_price.saturating_add(change_amount);
            }
            std::cmp::Ordering::Less => {
                let change_amount = self.exec_change(scaled_exec_gas_price);
                scaled_exec_gas_price =
                    scaled_exec_gas_price.saturating_sub(change_amount);
            }
            std::cmp::Ordering::Equal => {}
        }
        self.new_scaled_exec_price =
            max(self.min_scaled_exec_gas_price(), scaled_exec_gas_price);
    }

    fn min_scaled_exec_gas_price(&self) -> u64 {
        self.min_exec_gas_price
            .saturating_mul(self.gas_price_factor.into())
    }

    fn update_da_gas_price(&mut self) {
        let p = self.p();
        let d = self.d();
        let da_change = self.da_change(p, d);
        let maybe_new_scaled_da_gas_price = i128::from(self.new_scaled_da_gas_price)
            .checked_add(da_change)
            .and_then(|x| u64::try_from(x).ok())
            .unwrap_or_else(|| {
                if da_change.is_positive() {
                    u64::MAX
                } else {
                    0u64
                }
            });
        self.new_scaled_da_gas_price = max(
            self.min_scaled_da_gas_price(),
            maybe_new_scaled_da_gas_price,
        );
    }

    fn min_scaled_da_gas_price(&self) -> u64 {
        self.min_da_gas_price
            .saturating_mul(self.gas_price_factor.into())
    }

    fn p(&self) -> i128 {
        let upcast_p = i128::from(self.da_p_component);
        let checked_p = self.last_profit.checked_div(upcast_p);
        // If the profit is positive, we want to decrease the gas price
        checked_p.unwrap_or(0).saturating_mul(-1)
    }

    fn d(&self) -> i128 {
        let upcast_d = i128::from(self.da_d_component);
        let slope = self.last_profit.saturating_sub(self.second_to_last_profit);
        let checked_d = slope.checked_div(upcast_d);
        // if the slope is positive, we want to decrease the gas price
        checked_d.unwrap_or(0).saturating_mul(-1)
    }

    fn da_change(&self, p: i128, d: i128) -> i128 {
        let pd_change = p.saturating_add(d);
        let upcast_percent = self.max_da_gas_price_change_percent.into();
        let max_change = self
            .new_scaled_da_gas_price
            .saturating_mul(upcast_percent)
            .saturating_div(100)
            .into();
        let clamped_change = pd_change.saturating_abs().min(max_change);
        pd_change.signum().saturating_mul(clamped_change)
    }

    fn exec_change(&self, principle: u64) -> u64 {
        principle
            .saturating_mul(self.exec_gas_price_change_percent as u64)
            .saturating_div(100)
    }

    fn da_block_update(
        &mut self,
        height: u32,
        block_bytes: u64,
        block_cost: u64,
    ) -> Result<(), Error> {
        let expected = self.da_recorded_block_height.saturating_add(1);
        if height != expected {
            Err(Error::SkippedDABlock {
                expected: self.da_recorded_block_height.saturating_add(1),
                got: height,
            })
        } else {
            let new_cost_per_byte: u128 = (block_cost as u128)
                .checked_div(block_bytes as u128)
                .ok_or(Error::CouldNotCalculateCostPerByte {
                    bytes: block_bytes,
                    cost: block_cost,
                })?;
            self.da_recorded_block_height = height;
            let new_block_cost = self
                .latest_known_total_da_cost_excess
                .saturating_add(block_cost as u128);
            self.latest_known_total_da_cost_excess = new_block_cost;
            self.latest_da_cost_per_byte = new_cost_per_byte;
            Ok(())
        }
    }

    fn recalculate_projected_cost(&mut self) {
        // remove all blocks that have been recorded
        self.unrecorded_blocks
            .retain(|block| block.height > self.da_recorded_block_height);
        // add the cost of the remaining blocks
        let projection_portion: u128 = self
            .unrecorded_blocks
            .iter()
            .map(|block| {
                (block.block_bytes as u128).saturating_mul(self.latest_da_cost_per_byte)
            })
            .sum();
        self.projected_total_da_cost = self
            .latest_known_total_da_cost_excess
            .saturating_add(projection_portion);
    }

    fn descaled_exec_price(&self) -> u64 {
        self.new_scaled_exec_price.div(self.gas_price_factor)
    }

    fn descaled_da_price(&self) -> u64 {
        self.new_scaled_da_gas_price.div(self.gas_price_factor)
    }

    pub fn algorithm(&self) -> AlgorithmV1 {
        AlgorithmV1 {
            new_exec_price: self.descaled_exec_price(),
            exec_price_percentage: self.exec_gas_price_change_percent as u64,
            new_da_gas_price: self.descaled_da_price(),
            da_gas_price_percentage: self.max_da_gas_price_change_percent as u64,
            for_height: self.l2_block_height,
        }
    }

    // We only need to track the difference between the rewards and costs after we have true DA data
    // Normalize, or zero out the lower value and subtract it from the higher value
    fn normalize_rewards_and_costs(&mut self) {
        let (excess, projected_cost_excess) =
            if self.total_da_rewards_excess > self.latest_known_total_da_cost_excess {
                (
                    self.total_da_rewards_excess
                        .saturating_sub(self.latest_known_total_da_cost_excess),
                    self.projected_total_da_cost
                        .saturating_sub(self.latest_known_total_da_cost_excess),
                )
            } else {
                (
                    self.latest_known_total_da_cost_excess
                        .saturating_sub(self.total_da_rewards_excess),
                    self.projected_total_da_cost
                        .saturating_sub(self.total_da_rewards_excess),
                )
            };

        self.projected_total_da_cost = projected_cost_excess;
        if self.total_da_rewards_excess > self.latest_known_total_da_cost_excess {
            self.total_da_rewards_excess = excess;
            self.latest_known_total_da_cost_excess = 0;
        } else {
            self.total_da_rewards_excess = 0;
            self.latest_known_total_da_cost_excess = excess;
        }
    }
}
