#![allow(non_snake_case)]
use super::*;

struct UpdaterBuilder {
    starting_exec_gas_price: u64,
    starting_da_gas_price: u64,
    exec_gas_price_increase_amount: u64,
    da_p_component: i64,
    da_d_component: i64,

    l2_block_height: u32,
    l2_block_capacity_threshold: u64,

    total_rewards: u64,
    da_recorded_block_height: u32,
    da_cost_per_byte: u64,
    project_total_cost: u64,
    latest_known_total_cost: u64,
    unrecorded_blocks: Vec<BlockBytes>,
    last_profit: i64,
}

impl UpdaterBuilder {
    fn new() -> Self {
        Self {
            starting_exec_gas_price: 0,
            starting_da_gas_price: 0,
            exec_gas_price_increase_amount: 0,
            da_p_component: 0,
            da_d_component: 0,

            l2_block_height: 0,
            l2_block_capacity_threshold: 50,

            total_rewards: 0,
            da_recorded_block_height: 0,
            da_cost_per_byte: 0,
            project_total_cost: 0,
            latest_known_total_cost: 0,
            unrecorded_blocks: vec![],
            last_profit: 0,
        }
    }

    fn with_starting_exec_gas_price(mut self, starting_da_gas_price: u64) -> Self {
        self.starting_exec_gas_price = starting_da_gas_price;
        self
    }

    fn with_starting_da_gas_price(mut self, starting_da_gas_price: u64) -> Self {
        self.starting_da_gas_price = starting_da_gas_price;
        self
    }

    fn with_exec_gas_price_increase_amount(
        mut self,
        exec_gas_price_increase_amount: u64,
    ) -> Self {
        self.exec_gas_price_increase_amount = exec_gas_price_increase_amount;
        self
    }

    fn with_da_p_component(mut self, da_p_component: i64) -> Self {
        self.da_p_component = da_p_component;
        self
    }

    fn with_l2_block_height(mut self, starting_block: u32) -> Self {
        self.l2_block_height = starting_block;
        self
    }

    fn with_l2_block_capacity_threshold(
        mut self,
        l2_block_capacity_threshold: u64,
    ) -> Self {
        self.l2_block_capacity_threshold = l2_block_capacity_threshold;
        self
    }

    fn with_total_rewards(mut self, total_rewards: u64) -> Self {
        self.total_rewards = total_rewards;
        self
    }

    fn with_da_recorded_block_height(mut self, da_recorded_block_height: u32) -> Self {
        self.da_recorded_block_height = da_recorded_block_height;
        self
    }

    fn with_da_cost_per_byte(mut self, da_cost_per_byte: u64) -> Self {
        self.da_cost_per_byte = da_cost_per_byte;
        self
    }

    fn with_projected_total_cost(mut self, projected_total_cost: u64) -> Self {
        self.project_total_cost = projected_total_cost;
        self
    }

    fn with_known_total_cost(mut self, latest_known_total_cost: u64) -> Self {
        self.latest_known_total_cost = latest_known_total_cost;
        self
    }

    fn with_unrecorded_blocks(mut self, unrecorded_blocks: Vec<BlockBytes>) -> Self {
        self.unrecorded_blocks = unrecorded_blocks;
        self
    }

    fn with_last_profit(mut self, last_profit: i64) -> Self {
        self.last_profit = last_profit;
        self
    }

    fn build(self) -> AlgorithmUpdaterV1 {
        AlgorithmUpdaterV1 {
            new_exec_price: self.starting_exec_gas_price,
            last_da_price: self.starting_da_gas_price,
            exec_gas_price_increase_amount: self.exec_gas_price_increase_amount,
            da_p_component: self.da_p_component,
            da_d_component: 0,

            l2_block_height: self.l2_block_height,
            l2_block_fullness_threshold_percent: self.l2_block_capacity_threshold,
            total_da_rewards: self.total_rewards,

            da_recorded_block_height: self.da_recorded_block_height,
            latest_da_cost_per_byte: self.da_cost_per_byte,
            projected_total_da_cost: self.project_total_cost,
            latest_known_total_da_cost: self.latest_known_total_cost,
            unrecorded_blocks: self.unrecorded_blocks,
            last_profit: self.last_profit,
        }
    }
}

#[cfg(test)]
mod update_l2_block_data_tests;

#[cfg(test)]
mod update_da_record_data_tests;

#[cfg(test)]
mod algorithm_v1_tests;

#[ignore]
#[test]
fn dummy() {}
