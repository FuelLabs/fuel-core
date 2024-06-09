#![allow(non_snake_case)]
use super::*;

struct UpdaterBuilder {
    starting_block: u32,
    da_recorded_block_height: u32,
    da_cost_per_byte: u64,
    project_total_cost: u64,
    latest_known_total_cost: u64,
    unrecorded_blocks: Vec<BlockBytes>,
}

impl UpdaterBuilder {
    fn new() -> Self {
        Self {
            starting_block: 0,
            da_recorded_block_height: 0,
            da_cost_per_byte: 0,
            project_total_cost: 0,
            latest_known_total_cost: 0,
            unrecorded_blocks: vec![],
        }
    }

    fn with_l2_block_height(mut self, starting_block: u32) -> Self {
        self.starting_block = starting_block;
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


    fn build(self) -> AlgorithmUpdaterV1 {
        AlgorithmUpdaterV1 {
            l2_block_height: self.starting_block,
            da_recorded_block_height: self.da_recorded_block_height,
            latest_da_cost_per_byte: self.da_cost_per_byte,
            projected_total_cost: self.project_total_cost,
            latest_known_total_cost: self.latest_known_total_cost,
            unrecorded_blocks: self.unrecorded_blocks,
        }
    }
}

#[cfg(test)]
mod update_l2_block_data_tests;

#[cfg(test)]
mod update_da_record_data_tests;


#[ignore]
#[test]
fn dummy() {

}