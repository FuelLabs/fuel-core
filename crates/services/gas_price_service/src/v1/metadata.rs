use fuel_gas_price_algorithm::v1::{
    AlgorithmUpdaterV1,
    ClampedPercentage,
};
use std::num::NonZeroU64;

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub struct V1Metadata {
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
}

pub struct V1MetadataInitializer {
    new_exec_gas_price: u64,
    min_exec_gas_price: u64,
    exec_gas_price_change_percent: u16,
    l2_block_fullness_threshold_percent: u8,
    gas_price_factor: u64,
    min_da_gas_price: u64,
    max_da_gas_price_change_percent: u16,
    da_p_component: i64,
    da_d_component: i64,
}

impl V1MetadataInitializer {
    pub fn initialize(
        &self,
        l2_block_height: u32,
        da_block_height: u32,
    ) -> anyhow::Result<V1Metadata> {
        let gas_price_factor = NonZeroU64::new(self.gas_price_factor)
            .ok_or_else(|| anyhow::anyhow!("gas_price_factor must be non-zero"))?;
        let metadata = V1Metadata {
            #[allow(clippy::arithmetic_side_effects)]
            new_scaled_exec_price: self.new_exec_gas_price * gas_price_factor.get(),
            min_exec_gas_price: self.min_exec_gas_price,
            exec_gas_price_change_percent: self.exec_gas_price_change_percent,
            l2_block_height,
            l2_block_fullness_threshold_percent: ClampedPercentage::new(
                self.l2_block_fullness_threshold_percent,
            ),
            #[allow(clippy::arithmetic_side_effects)]
            new_scaled_da_gas_price: self.min_da_gas_price * gas_price_factor.get(),
            gas_price_factor,
            min_da_gas_price: self.min_da_gas_price,
            max_da_gas_price_change_percent: self.max_da_gas_price_change_percent,
            total_da_rewards_excess: 0,
            da_recorded_block_height: da_block_height,
            latest_known_total_da_cost_excess: 0,
            projected_total_da_cost: 0,
            da_p_component: self.da_p_component,
            da_d_component: self.da_d_component,
            last_profit: 0,
            second_to_last_profit: 0,
            latest_da_cost_per_byte: 0,
        };

        Ok(metadata)
    }
}

impl From<V1Metadata> for AlgorithmUpdaterV1 {
    fn from(metadata: V1Metadata) -> Self {
        Self {
            new_scaled_exec_price: metadata.new_scaled_exec_price,
            min_exec_gas_price: metadata.min_exec_gas_price,
            exec_gas_price_change_percent: metadata.exec_gas_price_change_percent,
            l2_block_height: metadata.l2_block_height,
            l2_block_fullness_threshold_percent: metadata
                .l2_block_fullness_threshold_percent,
            new_scaled_da_gas_price: metadata.new_scaled_da_gas_price,
            gas_price_factor: metadata.gas_price_factor,
            min_da_gas_price: metadata.min_da_gas_price,
            max_da_gas_price_change_percent: metadata.max_da_gas_price_change_percent,
            total_da_rewards_excess: metadata.total_da_rewards_excess,
            da_recorded_block_height: metadata.da_recorded_block_height,
            latest_known_total_da_cost_excess: metadata.latest_known_total_da_cost_excess,
            projected_total_da_cost: metadata.projected_total_da_cost,
            da_p_component: metadata.da_p_component,
            da_d_component: metadata.da_d_component,
            last_profit: metadata.last_profit,
            second_to_last_profit: metadata.second_to_last_profit,
            latest_da_cost_per_byte: metadata.latest_da_cost_per_byte,
            unrecorded_blocks: Default::default(),
        }
    }
}

impl From<AlgorithmUpdaterV1> for V1Metadata {
    fn from(updater: AlgorithmUpdaterV1) -> Self {
        Self {
            new_scaled_exec_price: updater.new_scaled_exec_price,
            min_exec_gas_price: updater.min_exec_gas_price,
            exec_gas_price_change_percent: updater.exec_gas_price_change_percent,
            l2_block_height: updater.l2_block_height,
            l2_block_fullness_threshold_percent: updater
                .l2_block_fullness_threshold_percent,
            new_scaled_da_gas_price: updater.new_scaled_da_gas_price,
            gas_price_factor: updater.gas_price_factor,
            min_da_gas_price: updater.min_da_gas_price,
            max_da_gas_price_change_percent: updater.max_da_gas_price_change_percent,
            total_da_rewards_excess: updater.total_da_rewards_excess,
            da_recorded_block_height: updater.da_recorded_block_height,
            latest_known_total_da_cost_excess: updater.latest_known_total_da_cost_excess,
            projected_total_da_cost: updater.projected_total_da_cost,
            da_p_component: updater.da_p_component,
            da_d_component: updater.da_d_component,
            last_profit: updater.last_profit,
            second_to_last_profit: updater.second_to_last_profit,
            latest_da_cost_per_byte: updater.latest_da_cost_per_byte,
        }
    }
}
