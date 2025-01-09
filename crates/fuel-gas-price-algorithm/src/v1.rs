use crate::utils::cumulative_percentage_change;
use std::{
    cmp::{
        max,
        min,
    },
    collections::BTreeMap,
    num::NonZeroU64,
    ops::{
        Div,
        RangeInclusive,
    },
};

#[cfg(test)]
mod tests;

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum Error {
    #[error("Skipped L2 block update: expected {expected:?}, got {got:?}")]
    SkippedL2Block { expected: u32, got: u32 },
    #[error("Could not calculate cost per byte: {bytes:?} bytes, {cost:?} cost")]
    CouldNotCalculateCostPerByte { bytes: u128, cost: u128 },
    #[error("Failed to include L2 block data: {0}")]
    FailedToIncludeL2BlockData(String),
    #[error("L2 block expected but not found in unrecorded blocks: {height}")]
    L2BlockExpectedNotFound { height: u32 },
    #[error("Could not insert unrecorded block: {0}")]
    CouldNotInsertUnrecordedBlock(String),
    #[error("Could not remove unrecorded block: {0}")]
    CouldNotRemoveUnrecordedBlock(String),
}

// TODO: separate exec gas price and DA gas price into newtypes for clarity
//   https://github.com/FuelLabs/fuel-core/issues/2382
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

pub type Height = u32;
pub type Bytes = u64;

pub trait UnrecordedBlocks {
    fn insert(&mut self, height: Height, bytes: Bytes) -> Result<(), String>;

    fn remove(&mut self, height: &Height) -> Result<Option<Bytes>, String>;
}

impl UnrecordedBlocks for BTreeMap<Height, Bytes> {
    fn insert(&mut self, height: Height, bytes: Bytes) -> Result<(), String> {
        self.insert(height, bytes);
        Ok(())
    }

    fn remove(&mut self, height: &Height) -> Result<Option<Bytes>, String> {
        let value = self.remove(height);
        Ok(value)
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
    /// The gas price (scaled by the `gas_price_factor`) to cover the DA commitment of the next block
    pub new_scaled_da_gas_price: u64,
    /// Scale factor for the gas price.
    pub gas_price_factor: NonZeroU64,
    /// The lowest the algorithm allows the da gas price to go
    pub min_da_gas_price: u64,
    /// The highest the algorithm allows the da gas price to go
    pub max_da_gas_price: u64,
    /// The maximum percentage that the DA portion of the gas price can change in a single block
    ///   Using `u16` because it can go above 100% and possibly over 255%
    pub max_da_gas_price_change_percent: u16,
    /// The cumulative reward from the DA portion of the gas price
    pub total_da_rewards_excess: u128,
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
    /// Activity of L2
    pub l2_activity: L2ActivityTracker,
    /// Total unrecorded block bytes
    pub unrecorded_blocks_bytes: u128,
}

/// The `L2ActivityTracker` tracks the chain activity to determine a safety mode for setting the DA price.
///
/// Because the DA gas price can increase even when no-one is using the network, there is a potential
/// for a negative feedback loop to occur where the gas price increases, further decreasing activity
/// and increasing the gas price. The `L2ActivityTracker` is used to moderate changes to the DA
/// gas price based on the activity of the L2 chain.
///
/// The chain activity is a cumulative measure, updated whenever a new block is processed.
/// For each L2 block, the block usage is a percentage of the block capacity used. If the
/// block usage is below a certain threshold, the chain activity is decreased, if above the threshold,
/// the activity is increased The chain activity exists on a scale
/// between 0 and the sum of the normal, capped, and decrease buffers.
///
/// e.g. if the decrease activity threshold is 20, the capped activity threshold is 80, and the max activity is 120,
/// we'd have the following ranges:
///
/// 0 <-- decrease range -->20<-- capped range -->80<-- normal range -->120
///
/// The current chain activity determines the behavior of the DA gas price.
///
/// For healthy behavior, the activity should be in the `normal` range.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub struct L2ActivityTracker {
    /// The maximum value the chain activity can hit
    max_activity: u16,
    /// The threshold if the block activity is below, the DA gas price will be held when it would otherwise be increased
    capped_activity_threshold: u16,
    /// If the chain activity falls below this value, the DA gas price will be decreased when it would otherwise be increased
    decrease_activity_threshold: u16,
    /// The current activity of the L2 chain
    chain_activity: u16,
    /// The threshold of block activity below which the chain activity will be decreased,
    /// above or equal it will always increase
    block_activity_threshold: ClampedPercentage,
}

/// Designates the intended behavior of the DA gas price based on the activity of the L2 chain
pub enum DAGasPriceSafetyMode {
    /// Should increase DA gas price freely
    Normal,
    /// Should not increase the DA gas price
    Capped,
    /// Should decrease the DA gas price always
    AlwaysDecrease,
}

impl L2ActivityTracker {
    pub fn new_full(
        normal_range_size: u16,
        capped_range_size: u16,
        decrease_range_size: u16,
        block_activity_threshold: ClampedPercentage,
    ) -> Self {
        let decrease_activity_threshold = decrease_range_size;
        let capped_activity_threshold =
            decrease_range_size.saturating_add(capped_range_size);
        let max_activity = capped_activity_threshold.saturating_add(normal_range_size);
        let chain_activity = max_activity;
        Self {
            max_activity,
            capped_activity_threshold,
            decrease_activity_threshold,
            chain_activity,
            block_activity_threshold,
        }
    }

    pub fn new(
        normal_range_size: u16,
        capped_range_size: u16,
        decrease_range_size: u16,
        activity: u16,
        block_activity_threshold: ClampedPercentage,
    ) -> Self {
        let mut tracker = Self::new_full(
            normal_range_size,
            capped_range_size,
            decrease_range_size,
            block_activity_threshold,
        );
        tracker.chain_activity = activity.min(tracker.max_activity);
        tracker
    }

    pub fn new_always_normal() -> Self {
        let normal_range_size = 100;
        let capped_range_size = 0;
        let decrease_range_size = 0;
        let percentage = ClampedPercentage::new(0);
        Self::new(
            normal_range_size,
            capped_range_size,
            decrease_range_size,
            100,
            percentage,
        )
    }

    pub fn safety_mode(&self) -> DAGasPriceSafetyMode {
        if self.chain_activity >= self.capped_activity_threshold {
            DAGasPriceSafetyMode::Normal
        } else if self.chain_activity >= self.decrease_activity_threshold {
            DAGasPriceSafetyMode::Capped
        } else {
            DAGasPriceSafetyMode::AlwaysDecrease
        }
    }

    pub fn update(&mut self, block_usage: ClampedPercentage) {
        if block_usage < self.block_activity_threshold {
            tracing::debug!(
                "Decreasing activity {:?} < {:?}",
                block_usage,
                self.block_activity_threshold
            );
            self.chain_activity = self.chain_activity.saturating_sub(1);
        } else {
            self.chain_activity =
                self.chain_activity.saturating_add(1).min(self.max_activity);
        }
    }

    pub fn current_activity(&self) -> u16 {
        self.chain_activity
    }

    pub fn max_activity(&self) -> u16 {
        self.max_activity
    }

    pub fn capped_activity_threshold(&self) -> u16 {
        self.capped_activity_threshold
    }

    pub fn decrease_activity_threshold(&self) -> u16 {
        self.decrease_activity_threshold
    }

    pub fn block_activity_threshold(&self) -> ClampedPercentage {
        self.block_activity_threshold
    }
}

/// A value that represents a value between 0 and 100. Higher values are clamped to 100
#[derive(
    serde::Serialize, serde::Deserialize, Debug, Copy, Clone, PartialEq, PartialOrd,
)]
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

impl AlgorithmUpdaterV1 {
    pub fn update_da_record_data<U: UnrecordedBlocks>(
        &mut self,
        heights: RangeInclusive<u32>,
        recorded_bytes: u32,
        recording_cost: u128,
        unrecorded_blocks: &mut U,
    ) -> Result<(), Error> {
        if !heights.is_empty() {
            self.da_block_update(
                heights,
                recorded_bytes as u128,
                recording_cost,
                unrecorded_blocks,
            )?;
            self.recalculate_projected_cost();
            self.update_da_gas_price();
        }
        Ok(())
    }

    pub fn update_l2_block_data<U: UnrecordedBlocks>(
        &mut self,
        height: u32,
        used: u64,
        capacity: NonZeroU64,
        block_bytes: u64,
        fee_wei: u128,
        unrecorded_blocks: &mut U,
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
            self.update_da_rewards(fee_wei);
            let rewards = self.clamped_rewards_as_i128();

            // costs
            self.update_projected_da_cost(block_bytes);
            let projected_total_da_cost = self.clamped_projected_cost_as_i128();

            // profit
            let last_profit = rewards.saturating_sub(projected_total_da_cost);
            self.update_last_profit(last_profit);

            // activity
            self.update_activity(used, capacity);

            // gas prices
            self.update_exec_gas_price(used, capacity);
            self.update_da_gas_price();

            // metadata
            unrecorded_blocks
                .insert(height, block_bytes)
                .map_err(Error::CouldNotInsertUnrecordedBlock)?;
            self.unrecorded_blocks_bytes = self
                .unrecorded_blocks_bytes
                .saturating_add(block_bytes as u128);
            Ok(())
        }
    }

    fn update_activity(&mut self, used: u64, capacity: NonZeroU64) {
        let block_activity = used.saturating_mul(100).div(capacity);
        let usage = ClampedPercentage::new(block_activity.try_into().unwrap_or(100));
        self.l2_activity.update(usage);
    }

    fn update_da_rewards(&mut self, fee_wei: u128) {
        let block_da_reward = self.da_portion_of_fee(fee_wei);
        self.total_da_rewards_excess =
            self.total_da_rewards_excess.saturating_add(block_da_reward);
    }

    fn update_projected_da_cost(&mut self, block_bytes: u64) {
        let block_projected_da_cost =
            (block_bytes as u128).saturating_mul(self.latest_da_cost_per_byte);
        self.projected_total_da_cost = self
            .projected_total_da_cost
            .saturating_add(block_projected_da_cost);
    }

    // Take the `fee_wei` and return the portion of the fee that should be used for paying DA costs
    fn da_portion_of_fee(&self, fee_wei: u128) -> u128 {
        // fee_wei * (da_price / (exec_price + da_price))
        let numerator = fee_wei.saturating_mul(self.descaled_da_price() as u128);
        let denominator = (self.descaled_exec_price() as u128)
            .saturating_add(self.descaled_da_price() as u128);
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
            std::cmp::Ordering::Greater | std::cmp::Ordering::Equal => {
                let change_amount = self.exec_change(scaled_exec_gas_price);
                scaled_exec_gas_price =
                    scaled_exec_gas_price.saturating_add(change_amount);
            }
            std::cmp::Ordering::Less => {
                let change_amount = self.exec_change(scaled_exec_gas_price);
                scaled_exec_gas_price =
                    scaled_exec_gas_price.saturating_sub(change_amount);
            }
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
        let maybe_scaled_da_change = self.da_change(p, d);
        let scaled_da_change =
            self.da_change_accounting_for_activity(maybe_scaled_da_change);
        let maybe_new_scaled_da_gas_price = i128::from(self.new_scaled_da_gas_price)
            .checked_add(scaled_da_change)
            .and_then(|x| u64::try_from(x).ok())
            .unwrap_or_else(|| {
                if scaled_da_change.is_positive() {
                    u64::MAX
                } else {
                    0u64
                }
            });
        tracing::debug!("Profit: {}", self.last_profit);
        tracing::debug!(
            "DA gas price change: p: {}, d: {}, change: {}, new: {}",
            p,
            d,
            scaled_da_change,
            maybe_new_scaled_da_gas_price
        );
        self.new_scaled_da_gas_price = min(
            max(
                self.min_scaled_da_gas_price(),
                maybe_new_scaled_da_gas_price,
            ),
            self.max_scaled_da_gas_price(),
        );
    }

    fn da_change_accounting_for_activity(&self, maybe_da_change: i128) -> i128 {
        if maybe_da_change > 0 {
            match self.l2_activity.safety_mode() {
                DAGasPriceSafetyMode::Normal => maybe_da_change,
                DAGasPriceSafetyMode::Capped => 0,
                DAGasPriceSafetyMode::AlwaysDecrease => {
                    tracing::info!("Activity is low, decreasing DA gas price");
                    self.max_change().saturating_mul(-1)
                }
            }
        } else {
            maybe_da_change
        }
    }

    fn min_scaled_da_gas_price(&self) -> u64 {
        self.min_da_gas_price
            .saturating_mul(self.gas_price_factor.into())
    }

    fn max_scaled_da_gas_price(&self) -> u64 {
        // note: here we make sure that a correct maximum is used; hacky :(
        max(self.max_da_gas_price, self.min_da_gas_price)
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
        let scaled_pd_change = p
            .saturating_add(d)
            .saturating_mul(self.gas_price_factor.get() as i128);
        let max_change = self.max_change();
        let clamped_change = scaled_pd_change.saturating_abs().min(max_change);
        scaled_pd_change.signum().saturating_mul(clamped_change)
    }

    // Should always be positive
    fn max_change(&self) -> i128 {
        let upcast_percent = self.max_da_gas_price_change_percent.into();
        self.new_scaled_da_gas_price
            .saturating_mul(upcast_percent)
            .saturating_div(100)
            .into()
    }

    fn exec_change(&self, principle: u64) -> u64 {
        principle
            .saturating_mul(self.exec_gas_price_change_percent as u64)
            .saturating_div(100)
    }

    fn da_block_update<U: UnrecordedBlocks>(
        &mut self,
        heights: RangeInclusive<u32>,
        recorded_bytes: u128,
        recording_cost: u128,
        unrecorded_blocks: &mut U,
    ) -> Result<(), Error> {
        self.update_unrecorded_block_bytes(heights, unrecorded_blocks)?;

        let new_da_block_cost = self
            .latest_known_total_da_cost_excess
            .saturating_add(recording_cost);
        self.latest_known_total_da_cost_excess = new_da_block_cost;

        let compressed_cost_per_bytes = recording_cost
            .checked_div(recorded_bytes)
            .ok_or(Error::CouldNotCalculateCostPerByte {
                bytes: recorded_bytes,
                cost: recording_cost,
            })?;

        // This is often "pessimistic" in the sense that we are charging for the compressed blocks
        // and we will use it to calculate base on the uncompressed blocks
        self.latest_da_cost_per_byte = compressed_cost_per_bytes;
        Ok(())
    }

    // Get the bytes for all specified heights, or get none of them.
    // Always remove the blocks from the unrecorded blocks so they don't build up indefinitely
    fn update_unrecorded_block_bytes<U: UnrecordedBlocks>(
        &mut self,
        heights: RangeInclusive<u32>,
        unrecorded_blocks: &mut U,
    ) -> Result<(), Error> {
        let mut total: u128 = 0;
        tracing::info!("removing for range: {:?}", heights);
        for expected_height in heights {
            let maybe_bytes = unrecorded_blocks
                .remove(&expected_height)
                .map_err(Error::CouldNotRemoveUnrecordedBlock)?;

            if let Some(bytes) = maybe_bytes {
                total = total.saturating_add(bytes as u128);
            } else {
                tracing::warn!(
                    "L2 block expected but not found in unrecorded blocks: {}",
                    expected_height,
                );
            }
        }
        tracing::info!("removing total: {}", total);
        self.unrecorded_blocks_bytes = self.unrecorded_blocks_bytes.saturating_sub(total);
        tracing::info!("unrecorded_blocks_bytes: {}", self.unrecorded_blocks_bytes);
        Ok(())
    }

    fn recalculate_projected_cost(&mut self) {
        let projection_portion = self
            .unrecorded_blocks_bytes
            .saturating_mul(self.latest_da_cost_per_byte);
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
}
