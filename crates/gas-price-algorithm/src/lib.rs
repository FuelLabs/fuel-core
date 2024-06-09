use std::{
    cell::RefCell,
    cmp::{
        max,
        min,
    },
};

#[cfg(test)]
mod tests;

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum Error {
    #[error("Skipped L2 block update: expected {expected:?}, got {got:?}")]
    SkippedL2Block{
        expected: u32,
        got: u32,
    },
    #[error("Skipped DA block update: expected {expected:?}, got {got:?}")]
    SkippedDABlock{
        expected: u32,
        got: u32,
    },
    #[error("Could not calculate cost per byte: {bytes:?} bytes, {cost:?} cost")]
    CouldNotCalculateCostPerByte {
        bytes: u64,
        cost: u64,
    }
}

pub struct AlgorithmV0 {
    // DA
    da_max_change_percent: u8,
    min_da_price: u64,
    da_p_value_factor: i64,
    da_d_value_factor: i64,
    moving_average_profit: RefCell<i64>,
    last_profit: RefCell<i64>,
    profit_slope: RefCell<i64>,
    moving_average_window: i64,
    // EXEC
    exec_change_amount: u64,
    min_exec_price: u64,
}

pub struct AlgorithmV1 {
    block_height: u32,
    price: u64,
    max_change_percent: u8,
}

// TODO: Add contstructor
pub struct AlgorithmUpdaterV1 {
    pub gas_price: u64,

    // L2
    pub l2_block_height: u32,
    pub l2_block_fullness_threshold_percent: u64,
    pub exec_gas_price_increase_amount: u64,
    pub total_rewards: u64,
    // total_reward: u64,
    // actual_total_cost: u64,
    // latest_gas_per_byte: u64,
    pub da_recorded_block_height: u32,
    pub latest_known_total_cost: u64,
    pub projected_total_cost: u64,

    pub latest_da_cost_per_byte: u64,
    pub unrecorded_blocks: Vec<BlockBytes>,
}

// TODO: Add contstructor
#[derive(Debug, Clone)]
pub struct RecordedBlock {
    pub height: u32,
    pub block_bytes: u64,
    pub block_cost: u64,
}

#[derive(Debug, Clone)]
pub struct BlockBytes {
    height: u32,
    block_bytes: u64,
}

impl AlgorithmUpdaterV1 {
    // pub fn new(l2_block_height: u32, da_recorded_block_height: u32, latest_da_cost_per_byte: u64) -> Self {
    //     Self {
    //         gas_price: 0,
    //         l2_block_height,
    //         l2_block_fullness_threshold: 0,
    //         exec_gas_price_increase_amount: 0,
    //         da_recorded_block_height,
    //         latest_known_total_cost: 0,
    //         projected_total_cost: 0,
    //         latest_da_cost_per_byte,
    //         unrecorded_blocks: Vec::new(),
    //     }
    // }

    pub fn update_da_record_data(&mut self, blocks: Vec<RecordedBlock>) -> Result<(), Error> {
        for block in blocks {
            self.da_block_update(block.height, block.block_bytes, block.block_cost)?;
        }
        self.recalculate_projected_cost();
        Ok(())
    }
    pub fn update_l2_block_data(&mut self, height: u32, fullness: (u64, u64), block_bytes: u64) -> Result<(), Error> {
        let expected = self.l2_block_height.saturating_add(1);
        if height != expected {
            return Err(Error::SkippedL2Block {
                expected,
                got: height,
            })
        } else {
            self.l2_block_height = height;
            self.projected_total_cost += block_bytes * self.latest_da_cost_per_byte;
            let reward = fullness.0 * self.gas_price;
            self.total_rewards += reward;
            let fullness_percent = fullness.0 * 100 / fullness.1;
            if fullness_percent > self.l2_block_fullness_threshold_percent {
                self.gas_price += self.exec_gas_price_increase_amount;
            }
            Ok(())
        }
    }

    fn da_block_update(&mut self, height: u32, block_bytes: u64, block_cost: u64) -> Result<(), Error> {
        let expected = self.da_recorded_block_height.saturating_add(1);
        if height != expected {
            return Err(Error::SkippedDABlock {
                expected: self.da_recorded_block_height.saturating_add(1),
                got: height,
            })
        } else {
            let new_cost_per_byte = block_cost.checked_div(block_bytes).ok_or(Error::CouldNotCalculateCostPerByte {
                bytes: block_bytes,
                cost: block_cost,
            })?;
            self.da_recorded_block_height = height;
            self.latest_known_total_cost += block_cost;
            self.latest_da_cost_per_byte = new_cost_per_byte;
            Ok(())
        }
    }

    fn recalculate_projected_cost(&mut self) {
        // remove all blocks that have been recorded
        self.unrecorded_blocks.retain(|block| block.height > self.da_recorded_block_height);
        // add the cost of the remaining blocks
        let projection_portion: u64 = self.unrecorded_blocks.iter().map(|block| block.block_bytes * self.latest_da_cost_per_byte).sum();
        self.projected_total_cost = self.latest_known_total_cost + projection_portion;
    }

    pub fn gas_price(&self) -> u64 {
        self.gas_price
    }
}

impl AlgorithmV0 {
    pub fn new(
        da_max_change_percent: u8,
        min_da_price: u64,
        da_p_value_factor: i64,
        da_d_value_factor: i64,
        moving_average_window: i64,
        exec_change_amount: u64,
        min_exec_price: u64,
    ) -> Self {
        Self {
            da_max_change_percent,
            min_da_price,
            da_p_value_factor,
            da_d_value_factor,
            moving_average_profit: RefCell::new(0),
            last_profit: RefCell::new(0),
            profit_slope: RefCell::new(0),
            moving_average_window,
            exec_change_amount,
            min_exec_price,
        }
    }

    pub fn calculate_da_gas_price(
        &self,
        old_da_gas_price: u64,
        total_production_reward: u64,
        total_da_recording_cost: u64,
    ) -> u64 {
        let new_profit = total_production_reward as i64 - total_da_recording_cost as i64;
        self.calculate_new_moving_average(new_profit);
        self.calculate_profit_slope(*self.moving_average_profit.borrow());
        let avg_profit = *self.moving_average_profit.borrow();
        let slope = *self.profit_slope.borrow();

        let max_change = (old_da_gas_price
            .saturating_mul(self.da_max_change_percent as u64)
            / 100) as i64;

        // if p > 0 and dp/db > 0, decrease
        // if p > 0 and dp/db < 0, hold/moderate
        // if p < 0 and dp/db < 0, increase
        // if p < 0 and dp/db > 0, hold/moderate
        let p_comp = avg_profit / self.da_p_value_factor;
        let d_comp = slope / self.da_d_value_factor;
        let pd_change = p_comp + d_comp;
        let change = min(max_change, pd_change.abs());
        let sign = pd_change.signum();
        let change = change * sign;
        let new_da_gas_price = old_da_gas_price as i64 - change;
        max(new_da_gas_price, self.min_da_price as i64) as u64
    }

    pub fn calculate_exec_gas_price(
        &self,
        old_exec_gas_price: u64,
        used: u64,
        capacity: u64,
    ) -> u64 {
        // TODO: This could be more sophisticated, e.g. have target fullness, rather than hardcoding 50%.
        let new = match used.cmp(&(capacity / 2)) {
            std::cmp::Ordering::Greater => {
                old_exec_gas_price.saturating_add(self.exec_change_amount)
            }
            std::cmp::Ordering::Less => {
                old_exec_gas_price.saturating_sub(self.exec_change_amount)
            }
            std::cmp::Ordering::Equal => old_exec_gas_price,
        };
        max(new, self.min_exec_price)
    }

    fn calculate_new_moving_average(&self, new_profit: i64) {
        let old = *self.moving_average_profit.borrow();
        *self.moving_average_profit.borrow_mut() = (old
            * (self.moving_average_window - 1))
            .saturating_add(new_profit)
            .saturating_div(self.moving_average_window);
    }

    fn calculate_profit_slope(&self, new_profit: i64) {
        let old_profit = *self.last_profit.borrow();
        *self.profit_slope.borrow_mut() = new_profit - old_profit;
        *self.last_profit.borrow_mut() = new_profit;
    }
}
