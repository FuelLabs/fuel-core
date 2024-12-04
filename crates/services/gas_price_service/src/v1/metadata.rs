use crate::v0::metadata::V0Metadata;
use fuel_gas_price_algorithm::v1::{
    AlgorithmUpdaterV1,
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
    /// The cumulative cost of recording L2 blocks on the DA chain as of the last recorded block
    pub latest_known_total_da_cost_excess: u128,
    /// The last profit
    pub last_profit: i128,
    /// The profit before last
    pub second_to_last_profit: i128,
    /// The latest known cost per byte for recording blocks on the DA chain
    pub latest_da_cost_per_byte: u128,
    /// Track the total bytes of all l2 blocks that have not been recorded on the DA chain (that we know),
    /// but have been used to estimate the cost of recording blocks on the DA chain
    pub unrecorded_block_bytes: u128,
}

impl V1Metadata {
    pub fn construct_from_v0_metadata(
        v0_metadata: V0Metadata,
        config: &V1AlgorithmConfig,
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
            latest_known_total_da_cost_excess: 0,
            last_profit: 0,
            second_to_last_profit: 0,
            latest_da_cost_per_byte: 0,
            unrecorded_block_bytes: 0,
        };
        Ok(metadata)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct V1AlgorithmConfig {
    pub new_exec_gas_price: u64,
    pub min_exec_gas_price: u64,
    pub exec_gas_price_change_percent: u16,
    pub l2_block_fullness_threshold_percent: u8,
    pub gas_price_factor: NonZeroU64,
    pub min_da_gas_price: u64,
    pub max_da_gas_price_change_percent: u16,
    pub da_p_component: i64,
    pub da_d_component: i64,
    pub normal_range_size: u16,
    pub capped_range_size: u16,
    pub decrease_range_size: u16,
    pub block_activity_threshold: u8,
}

pub fn updater_from_config(value: &V1AlgorithmConfig) -> AlgorithmUpdaterV1 {
    let l2_activity = L2ActivityTracker::new_full(
        value.normal_range_size,
        value.capped_range_size,
        value.decrease_range_size,
        value.block_activity_threshold.into(),
    );
    let unrecorded_blocks_bytes = 0;
    AlgorithmUpdaterV1 {
        new_scaled_exec_price: value
            .new_exec_gas_price
            .saturating_mul(value.gas_price_factor.get()),
        l2_block_height: 0,
        new_scaled_da_gas_price: value.min_da_gas_price,
        gas_price_factor: value.gas_price_factor,
        total_da_rewards_excess: 0,
        latest_known_total_da_cost_excess: 0,
        projected_total_da_cost: 0,
        last_profit: 0,
        second_to_last_profit: 0,
        latest_da_cost_per_byte: 0,
        l2_activity,
        min_exec_gas_price: value.min_exec_gas_price,
        exec_gas_price_change_percent: value.exec_gas_price_change_percent,
        l2_block_fullness_threshold_percent: value
            .l2_block_fullness_threshold_percent
            .into(),
        min_da_gas_price: value.min_da_gas_price,
        max_da_gas_price_change_percent: value.max_da_gas_price_change_percent,
        da_p_component: value.da_p_component,
        da_d_component: value.da_d_component,
        unrecorded_blocks_bytes,
    }
}

impl From<AlgorithmUpdaterV1> for V1Metadata {
    fn from(updater: AlgorithmUpdaterV1) -> Self {
        Self {
            new_scaled_exec_price: updater.new_scaled_exec_price,
            l2_block_height: updater.l2_block_height,
            new_scaled_da_gas_price: updater.new_scaled_da_gas_price,
            gas_price_factor: updater.gas_price_factor,
            total_da_rewards_excess: updater.total_da_rewards_excess,
            latest_known_total_da_cost_excess: updater.latest_known_total_da_cost_excess,
            last_profit: updater.last_profit,
            second_to_last_profit: updater.second_to_last_profit,
            latest_da_cost_per_byte: updater.latest_da_cost_per_byte,
            unrecorded_block_bytes: updater.unrecorded_blocks_bytes,
        }
    }
}

pub fn v1_algorithm_from_metadata(
    metadata: V1Metadata,
    config: &V1AlgorithmConfig,
) -> AlgorithmUpdaterV1 {
    let l2_activity = L2ActivityTracker::new_full(
        config.normal_range_size,
        config.capped_range_size,
        config.decrease_range_size,
        config.block_activity_threshold.into(),
    );
    let unrecorded_blocks_bytes: u128 = metadata.unrecorded_block_bytes;
    let projected_portion =
        unrecorded_blocks_bytes.saturating_mul(metadata.latest_da_cost_per_byte);
    let projected_total_da_cost = metadata
        .latest_known_total_da_cost_excess
        .saturating_add(projected_portion);
    AlgorithmUpdaterV1 {
        new_scaled_exec_price: metadata.new_scaled_exec_price,
        l2_block_height: metadata.l2_block_height,
        new_scaled_da_gas_price: metadata.new_scaled_da_gas_price,
        gas_price_factor: metadata.gas_price_factor,
        total_da_rewards_excess: metadata.total_da_rewards_excess,
        latest_known_total_da_cost_excess: metadata.latest_known_total_da_cost_excess,
        projected_total_da_cost,
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
        unrecorded_blocks_bytes,
    }
}
