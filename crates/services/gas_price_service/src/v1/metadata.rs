use crate::v0::metadata::V0Metadata;
use fuel_gas_price_algorithm::v1::{
    AlgorithmUpdaterV1,
    ClampedPercentage,
    L2ActivityTracker,
};
use std::num::NonZeroU64;

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub struct V1Metadata {
    // Execution
    /// The gas price (scaled by the `gas_price_factor`) to cover the execution of the next block
    pub new_scaled_exec_price: u64,
    /// The height of the next L2 block
    pub l2_block_height: u32,
    // DA
    /// The gas price (scaled by the `gas_price_factor`) to cover the DA commitment of the next block
    pub new_scaled_da_gas_price: u64,
    /// Scale factor for the gas price.
    pub gas_price_factor: NonZeroU64,
    /// The cumulative reward from the DA portion of the gas price
    pub total_da_rewards_excess: u128,
    /// The height of the last L2 block recorded on the DA chain
    pub da_recorded_block_height: u32,
    /// The cumulative cost of recording L2 blocks on the DA chain as of the last recorded block
    pub latest_known_total_da_cost_excess: u128,
    /// The predicted cost of recording L2 blocks on the DA chain as of the last L2 block
    /// (This value is added on top of the `latest_known_total_da_cost` if the L2 height is higher)
    pub projected_total_da_cost: u128,
    /// The last profit
    pub last_profit: i128,
    /// The profit before last
    pub second_to_last_profit: i128,
    /// The latest known cost per byte for recording blocks on the DA chain
    pub latest_da_cost_per_byte: u128,
    /// List of (height, size) for l2 blocks that have not been recorded on the DA chain (that we know),
    /// but have been used to estimate the cost of recording blocks on the DA chain
    pub unrecorded_blocks: Vec<(u32, u64)>,
}

impl V1Metadata {
    pub fn construct_from_v0_metadata(
        v0_metadata: V0Metadata,
        config: V1AlgorithmConfig,
    ) -> anyhow::Result<Self> {
        let metadata = Self {
            new_scaled_exec_price: v0_metadata
                .new_exec_price
                .saturating_mul(config.gas_price_factor.get()),
            l2_block_height: v0_metadata.l2_block_height,
            new_scaled_da_gas_price: config
                .min_da_gas_price
                .saturating_mul(config.gas_price_factor.get()),
            gas_price_factor: config.gas_price_factor,
            total_da_rewards_excess: 0,
            // TODO: Set to `None` after:
            //   https://github.com/FuelLabs/fuel-core/issues/2397
            da_recorded_block_height: 0,
            latest_known_total_da_cost_excess: 0,
            projected_total_da_cost: 0,
            last_profit: 0,
            second_to_last_profit: 0,
            latest_da_cost_per_byte: 0,
            unrecorded_blocks: vec![],
        };
        Ok(metadata)
    }
}

pub struct V1AlgorithmConfig {
    new_exec_gas_price: u64,
    min_exec_gas_price: u64,
    exec_gas_price_change_percent: u16,
    l2_block_fullness_threshold_percent: u8,
    gas_price_factor: NonZeroU64,
    min_da_gas_price: u64,
    max_da_gas_price_change_percent: u16,
    da_p_component: i64,
    da_d_component: i64,
    normal_range_size: u16,
    capped_range_size: u16,
    decrease_range_size: u16,
    block_activity_threshold: u8,
}

impl From<AlgorithmUpdaterV1> for V1Metadata {
    fn from(updater: AlgorithmUpdaterV1) -> Self {
        Self {
            new_scaled_exec_price: updater.new_scaled_exec_price,
            l2_block_height: updater.l2_block_height,
            new_scaled_da_gas_price: updater.new_scaled_da_gas_price,
            gas_price_factor: updater.gas_price_factor,
            total_da_rewards_excess: updater.total_da_rewards_excess,
            da_recorded_block_height: updater.da_recorded_block_height,
            latest_known_total_da_cost_excess: updater.latest_known_total_da_cost_excess,
            projected_total_da_cost: updater.projected_total_da_cost,
            last_profit: updater.last_profit,
            second_to_last_profit: updater.second_to_last_profit,
            latest_da_cost_per_byte: updater.latest_da_cost_per_byte,
            unrecorded_blocks: updater.unrecorded_blocks.into_iter().collect(),
        }
    }
}

pub fn v1_algorithm_from_metadata(
    metadata: V1Metadata,
    config: V1AlgorithmConfig,
) -> AlgorithmUpdaterV1 {
    let l2_activity = L2ActivityTracker::new_full(
        config.normal_range_size,
        config.capped_range_size,
        config.decrease_range_size,
        config.block_activity_threshold.into(),
    );
    let unrecorded_blocks = metadata.unrecorded_blocks.into_iter().collect();
    AlgorithmUpdaterV1 {
        new_scaled_exec_price: metadata.new_scaled_exec_price,
        l2_block_height: metadata.l2_block_height,
        new_scaled_da_gas_price: metadata.new_scaled_da_gas_price,
        gas_price_factor: metadata.gas_price_factor,
        total_da_rewards_excess: metadata.total_da_rewards_excess,
        da_recorded_block_height: metadata.da_recorded_block_height,
        latest_known_total_da_cost_excess: metadata.latest_known_total_da_cost_excess,
        projected_total_da_cost: metadata.projected_total_da_cost,
        last_profit: metadata.last_profit,
        second_to_last_profit: metadata.second_to_last_profit,
        latest_da_cost_per_byte: metadata.latest_da_cost_per_byte,
        l2_activity,
        min_exec_gas_price: config.min_exec_gas_price,
        exec_gas_price_change_percent: config.exec_gas_price_change_percent,
        l2_block_fullness_threshold_percent: config
            .l2_block_fullness_threshold_percent
            .into(),
        min_da_gas_price: config.min_da_gas_price,
        max_da_gas_price_change_percent: config.max_da_gas_price_change_percent,
        da_p_component: config.da_p_component,
        da_d_component: config.da_d_component,
        unrecorded_blocks,
    }
}
