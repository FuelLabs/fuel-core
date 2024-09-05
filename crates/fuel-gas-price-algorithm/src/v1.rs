use std::{
    cmp::{
        max,
        min,
    },
    num::NonZeroU64,
};

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
}

/// An algorithm for calculating the gas price for the next block
///
/// The algorithm breaks up the gas price into two components:
/// - The execution gas price, which is used to cover the cost of executing the next block as well
///   as moderating the congestion of the network by increasing the price when traffic is high.
/// - The data availability (DA) gas price, which is used to cover the cost of recording the block on the DA chain
///
/// The execution gas price is calculated eagerly based on the fullness of the last received l2 block. Each
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
#[derive(Debug, Clone, PartialEq)]
pub struct AlgorithmV1 {
    /// The lowest the algorithm allows the gas price to go
    min_da_gas_price: u64,
    /// The gas price for to cover the execution of the next block
    new_exec_price: u64,
    /// The gas price for the DA portion of the last block. This can be used to calculate
    last_da_price: u64,
    /// The maximum percentage that the DA portion of the gas price can change in a single block
    max_change_percent: u8,
    /// The latest known cost per byte for recording blocks on the DA chain
    latest_da_cost_per_byte: u128,
    /// The cumulative reward from the DA portion of the gas price
    total_rewards: u64,
    /// The cumulative cost of recording L2 blocks on the DA chain as of the last recorded block
    total_costs: u128,
    /// The P component of the PID control for the DA gas price
    da_p_factor: i64,
    /// The D component of the PID control for the DA gas price
    da_d_factor: i64,
    /// The average profit over the last `avg_window` blocks
    last_profit: i64,
    /// the previous profit
    second_to_last_profit: i64,
}

impl AlgorithmV1 {
    pub fn calculate(&self, _block_bytes: u64) -> u64 {
        let p = self.p();
        let d = self.d();
        let da_change = self.change(p, d);

        self.assemble_price(da_change)
    }

    fn p(&self) -> i64 {
        let checked_p = self.last_profit.checked_div(self.da_p_factor);
        // If the profit is positive, we want to decrease the gas price
        checked_p.unwrap_or(0).saturating_mul(-1)
    }

    fn d(&self) -> i64 {
        let slope = self.last_profit.saturating_sub(self.second_to_last_profit);
        let checked_d = slope.checked_div(self.da_d_factor);
        // if the slope is positive, we want to decrease the gas price
        checked_d.unwrap_or(0).saturating_mul(-1)
    }

    fn change(&self, p: i64, d: i64) -> i64 {
        let pd_change = p.saturating_add(d);
        let max_change = self
            .last_da_price
            .saturating_mul(self.max_change_percent as u64)
            .saturating_div(100) as i64;
        let sign = pd_change.signum();
        let signless_da_change = min(max_change, pd_change.abs());
        sign.saturating_mul(signless_da_change)
    }

    fn assemble_price(&self, change: i64) -> u64 {
        let last_da_gas_price = self.last_da_price as i128;
        let maybe_new_da_gas_price = last_da_gas_price
            .saturating_add(change as i128)
            .try_into()
            .unwrap_or(self.min_da_gas_price);
        let new_da_gas_price = max(self.min_da_gas_price, maybe_new_da_gas_price);
        // println!(
        //     "exec price: {}, da price: {}",
        //     self.new_exec_price, new_da_gas_price
        // );
        self.new_exec_price.saturating_add(new_da_gas_price)
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
pub struct AlgorithmUpdaterV1 {
    // Execution
    /// The gas price to cover the execution of the next block
    pub new_scaled_exec_price: u64,
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
    // DA
    /// The gas price for the DA portion of the last block. This can be used to calculate
    /// the DA portion of the next block
    pub last_da_gas_price: u64,
    /// Scale factor for the gas price.
    pub gas_price_factor: NonZeroU64,
    /// The lowest the algorithm allows the da gas price to go
    pub min_da_gas_price: u64,
    /// The maximum percentage that the DA portion of the gas price can change in a single block
    pub max_da_gas_price_change_percent: u8,
    /// The cumulative reward from the DA portion of the gas price
    pub total_da_rewards: u64,
    /// The height of the las L2 block recorded on the DA chain
    pub da_recorded_block_height: u32,
    /// The cumulative cost of recording L2 blocks on the DA chain as of the last recorded block
    pub latest_known_total_da_cost: u128,
    /// The predicted cost of recording L2 blocks on the DA chain as of the last L2 block
    /// (This value is added on top of the `latest_known_total_da_cost` if the L2 height is higher)
    pub projected_total_da_cost: u128,
    /// The P component of the PID control for the DA gas price
    pub da_p_component: i64,
    /// The D component of the PID control for the DA gas price
    pub da_d_component: i64,
    /// The last profit
    pub last_profit: i64,
    /// The profit before last
    pub second_to_last_profit: i64,
    /// The latest known cost per byte for recording blocks on the DA chain
    pub latest_da_cost_per_byte: u128,
    /// The unrecorded blocks that are used to calculate the projected cost of recording blocks
    pub unrecorded_blocks: Vec<BlockBytes>,
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
        blocks: Vec<RecordedBlock>,
    ) -> Result<(), Error> {
        for block in blocks {
            self.da_block_update(block.height, block.block_bytes, block.block_cost)?;
        }
        self.recalculate_projected_cost();
        Ok(())
    }

    pub fn update_l2_block_data(
        &mut self,
        height: u32,
        used: u64,
        capacity: NonZeroU64,
        block_bytes: u64,
        gas_price: u64,
    ) -> Result<(), Error> {
        let expected = self.l2_block_height.saturating_add(1);
        if height != expected {
            Err(Error::SkippedL2Block {
                expected,
                got: height,
            })
        } else {
            self.l2_block_height = height;
            let last_exec_price = self
                .new_scaled_exec_price
                .saturating_div(self.gas_price_factor.into());
            let last_profit = (self.total_da_rewards as i64)
                .saturating_sub(self.projected_total_da_cost as i64);
            self.update_last_profit(last_profit);
            #[allow(clippy::arithmetic_side_effects)]
            let block_projected_da_cost =
                (block_bytes as u128).saturating_mul(self.latest_da_cost_per_byte);
            self.projected_total_da_cost = self
                .projected_total_da_cost
                .saturating_add(block_projected_da_cost);
            // implicitly deduce what our da gas price was for the l2 block
            self.last_da_gas_price = gas_price.saturating_sub(last_exec_price);
            // println!(
            //     "gas price: {}, last exec price: {}, implicit da gas price: {}",
            //     gas_price, last_exec_price, self.last_da_gas_price
            // );
            self.update_exec_gas_price(used, capacity);
            #[allow(clippy::arithmetic_side_effects)]
            // the `da_gas_price_factor` will never be `0`
            let block_da_reward = used.saturating_mul(self.last_da_gas_price);
            // println!(
            //     "block project cost: {}, block reward: {}",
            //     block_projected_da_cost, block_da_reward
            // );
            self.total_da_rewards = self.total_da_rewards.saturating_add(block_da_reward);
            Ok(())
        }
    }

    fn update_last_profit(&mut self, new_profit: i64) {
        self.second_to_last_profit = self.last_profit;
        self.last_profit = new_profit;
    }

    fn update_exec_gas_price(&mut self, used: u64, capacity: NonZeroU64) {
        let mut exec_gas_price = self.new_scaled_exec_price;
        let fullness_percent = used
            .saturating_mul(100)
            .checked_div(capacity.into())
            .unwrap_or(self.l2_block_fullness_threshold_percent);

        match fullness_percent.cmp(&self.l2_block_fullness_threshold_percent) {
            std::cmp::Ordering::Greater => {
                let change_amount = self.change_amount(exec_gas_price);
                exec_gas_price = exec_gas_price.saturating_add(change_amount);
            }
            std::cmp::Ordering::Less => {
                let change_amount = self.change_amount(exec_gas_price);
                exec_gas_price = exec_gas_price.saturating_sub(change_amount);
            }
            std::cmp::Ordering::Equal => {}
        }
        self.new_scaled_exec_price = max(self.min_exec_gas_price, exec_gas_price);
    }

    fn change_amount(&self, principle: u64) -> u64 {
        principle
            .saturating_mul(self.exec_gas_price_change_percent)
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
                .checked_div(u64::from(block_bytes) as u128)
                .ok_or(Error::CouldNotCalculateCostPerByte {
                    bytes: block_bytes,
                    cost: block_cost,
                })?;
            self.da_recorded_block_height = height;
            let new_block_cost = self
                .latest_known_total_da_cost
                .saturating_add(block_cost as u128);
            self.latest_known_total_da_cost = new_block_cost;
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
            .latest_known_total_da_cost
            .saturating_add(projection_portion);
    }

    pub fn algorithm(&self) -> AlgorithmV1 {
        AlgorithmV1 {
            min_da_gas_price: self.min_da_gas_price,
            new_exec_price: self
                .new_scaled_exec_price
                .saturating_div(self.gas_price_factor.into()),
            last_da_price: self.last_da_gas_price,
            max_change_percent: self.max_da_gas_price_change_percent,

            latest_da_cost_per_byte: self.latest_da_cost_per_byte,
            total_rewards: self.total_da_rewards,
            total_costs: self.projected_total_da_cost,
            last_profit: self.last_profit,
            second_to_last_profit: self.second_to_last_profit,
            da_p_factor: self.da_p_component,
            da_d_factor: self.da_d_component,
        }
    }
}
