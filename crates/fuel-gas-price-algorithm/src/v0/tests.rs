#![allow(non_snake_case)]
#![allow(clippy::arithmetic_side_effects)]
#![allow(clippy::cast_possible_truncation)]

use super::AlgorithmUpdaterV0;

#[cfg(test)]
mod algorithm_v0_tests;
#[cfg(test)]
mod update_l2_block_data_tests;

pub struct UpdaterBuilder {
    min_exec_gas_price: u64,
    starting_exec_gas_price: u64,
    exec_gas_price_change_percent: u64,
    l2_block_height: u32,
    l2_block_capacity_threshold: u64,
}

impl UpdaterBuilder {
    fn new() -> Self {
        Self {
            min_exec_gas_price: 0,
            starting_exec_gas_price: 0,
            exec_gas_price_change_percent: 0,
            l2_block_height: 0,
            l2_block_capacity_threshold: 50,
        }
    }

    fn with_min_exec_gas_price(mut self, min_price: u64) -> Self {
        self.min_exec_gas_price = min_price;
        self
    }

    fn with_starting_exec_gas_price(mut self, starting_da_gas_price: u64) -> Self {
        self.starting_exec_gas_price = starting_da_gas_price;
        self
    }

    fn with_exec_gas_price_change_percent(mut self, percent: u64) -> Self {
        self.exec_gas_price_change_percent = percent;
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

    fn build(self) -> AlgorithmUpdaterV0 {
        AlgorithmUpdaterV0 {
            min_exec_gas_price: self.min_exec_gas_price,
            new_exec_price: self.starting_exec_gas_price,
            exec_gas_price_change_percent: self.exec_gas_price_change_percent,
            l2_block_height: self.l2_block_height,
            l2_block_fullness_threshold_percent: self.l2_block_capacity_threshold,
        }
    }
}

#[ignore]
#[test]
fn dummy() {}
