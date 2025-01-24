#![allow(non_snake_case)]
#![allow(clippy::arithmetic_side_effects)]
#![allow(clippy::cast_possible_truncation)]

use crate::v1::{
    AlgorithmUpdaterV1,
    Bytes,
    Height,
    L2ActivityTracker,
};
use std::collections::BTreeMap;

#[cfg(test)]
mod algorithm_v1_tests;
#[cfg(test)]
mod update_da_record_data_tests;
#[cfg(test)]
mod update_l2_block_data_tests;

#[derive(Debug, Clone)]
pub struct BlockBytes {
    pub height: u32,
    pub block_bytes: u64,
}

pub struct UpdaterBuilder {
    min_exec_gas_price: u64,
    min_da_gas_price: u64,
    max_da_gas_price: u64,
    starting_exec_gas_price: u64,
    starting_da_gas_price: u64,
    exec_gas_price_change_percent: u16,
    max_da_change_percent: u16,

    da_p_component: i64,
    da_d_component: i64,

    l2_block_height: u32,
    l2_block_capacity_threshold: u8,

    total_rewards: u128,
    da_cost_per_byte: u128,
    project_total_cost: u128,
    latest_known_total_cost: u128,
    unrecorded_blocks_bytes: u64,
    last_profit: i128,
    second_to_last_profit: i128,
    da_gas_price_factor: u64,
    l2_activity: L2ActivityTracker,
}

impl UpdaterBuilder {
    fn new() -> Self {
        Self {
            min_exec_gas_price: 0,
            min_da_gas_price: 0,
            max_da_gas_price: u64::MAX,
            starting_exec_gas_price: 1,
            starting_da_gas_price: 1,
            exec_gas_price_change_percent: 0,
            max_da_change_percent: u16::MAX,

            da_p_component: 0,
            da_d_component: 0,

            l2_block_height: 0,
            l2_block_capacity_threshold: 50,

            total_rewards: 0,
            da_cost_per_byte: 0,
            project_total_cost: 0,
            latest_known_total_cost: 0,
            unrecorded_blocks_bytes: 0,
            last_profit: 0,
            second_to_last_profit: 0,
            da_gas_price_factor: 1,
            l2_activity: L2ActivityTracker::new_always_normal(),
        }
    }

    fn with_min_exec_gas_price(mut self, min_price: u64) -> Self {
        self.min_exec_gas_price = min_price;
        self
    }

    fn with_min_da_gas_price(mut self, min_price: u64) -> Self {
        self.min_da_gas_price = min_price;
        self
    }

    fn with_max_da_gas_price(mut self, max_price: u64) -> Self {
        self.max_da_gas_price = max_price;
        self
    }

    fn with_starting_exec_gas_price(mut self, starting_da_gas_price: u64) -> Self {
        self.starting_exec_gas_price = starting_da_gas_price;
        self
    }

    fn with_starting_da_gas_price(mut self, starting_da_gas_price: u64) -> Self {
        self.starting_da_gas_price = starting_da_gas_price;
        self
    }

    fn with_exec_gas_price_change_percent(mut self, percent: u16) -> Self {
        self.exec_gas_price_change_percent = percent;
        self
    }

    fn with_da_max_change_percent(mut self, max_change_percent: u16) -> Self {
        self.max_da_change_percent = max_change_percent;
        self
    }

    fn with_da_p_component(mut self, da_p_component: i64) -> Self {
        self.da_p_component = da_p_component;
        self
    }

    fn with_da_d_component(mut self, da_d_component: i64) -> Self {
        self.da_d_component = da_d_component;
        self
    }

    fn with_l2_block_height(mut self, starting_block: u32) -> Self {
        self.l2_block_height = starting_block;
        self
    }

    fn with_l2_block_capacity_threshold(
        mut self,
        l2_block_capacity_threshold: u8,
    ) -> Self {
        self.l2_block_capacity_threshold = l2_block_capacity_threshold;
        self
    }

    fn with_total_rewards(mut self, total_rewards: u128) -> Self {
        self.total_rewards = total_rewards;
        self
    }

    fn with_da_cost_per_byte(mut self, da_cost_per_byte: u128) -> Self {
        self.da_cost_per_byte = da_cost_per_byte;
        self
    }

    fn with_projected_total_cost(mut self, projected_total_cost: u128) -> Self {
        self.project_total_cost = projected_total_cost;
        self
    }

    fn with_known_total_cost(mut self, latest_known_total_cost: u128) -> Self {
        self.latest_known_total_cost = latest_known_total_cost;
        self
    }

    fn with_unrecorded_blocks(
        mut self,
        unrecorded_blocks: &BTreeMap<Height, Bytes>,
    ) -> Self {
        let unrecorded_block_bytes = unrecorded_blocks
            .iter()
            .fold(0u64, |acc, (_, bytes)| acc + bytes);
        self.unrecorded_blocks_bytes = unrecorded_block_bytes;
        self
    }

    fn with_last_profit(mut self, last_profit: i128, last_last_profit: i128) -> Self {
        self.last_profit = last_profit;
        self.second_to_last_profit = last_last_profit;
        self
    }

    fn with_activity(mut self, l2_activity: L2ActivityTracker) -> Self {
        self.l2_activity = l2_activity;
        self
    }

    fn build(self) -> AlgorithmUpdaterV1 {
        AlgorithmUpdaterV1 {
            min_exec_gas_price: self.min_exec_gas_price,
            new_scaled_exec_price: self.starting_exec_gas_price,
            new_scaled_da_gas_price: self.starting_da_gas_price,
            exec_gas_price_change_percent: self.exec_gas_price_change_percent,
            max_da_gas_price_change_percent: self.max_da_change_percent,

            da_p_component: self.da_p_component,
            da_d_component: self.da_d_component,

            l2_block_height: self.l2_block_height,
            l2_block_fullness_threshold_percent: self.l2_block_capacity_threshold.into(),
            total_da_rewards: self.total_rewards,

            latest_da_cost_per_byte: self.da_cost_per_byte,
            projected_total_da_cost: self.project_total_cost,
            latest_known_total_da_cost: self.latest_known_total_cost,
            last_profit: self.last_profit,
            second_to_last_profit: self.second_to_last_profit,
            min_da_gas_price: self.min_da_gas_price,
            max_da_gas_price: self.max_da_gas_price,
            gas_price_factor: self
                .da_gas_price_factor
                .try_into()
                .expect("Should never be non-zero"),
            l2_activity: self.l2_activity,
            unrecorded_blocks_bytes: self.unrecorded_blocks_bytes as u128,
        }
    }
}

#[ignore]
#[test]
fn dummy() {}
