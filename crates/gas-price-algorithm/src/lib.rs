use std::cmp::{
    max,
    min,
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

pub struct AlgorithmV1 {
    new_exec_price: u64,
    last_da_price: u64,
    max_change_percent: u8,

    // DA
    latest_da_cost_per_byte: u64,
    total_rewards: u64,
    total_costs: u64,
    da_p_component: i64,
    da_d_component: i64,
    avg_profit: i64,
    avg_window: u32,
}

impl AlgorithmV1 {
    pub fn calculate(&self, block_bytes: u64) -> u64 {
        // DA PORTION
        let mut new_da_gas_price = self.last_da_price as i64;
        let extra_for_this_block = block_bytes * self.latest_da_cost_per_byte;
        // let extra_for_this_block = 0;
        let pessimistic_cost = self.total_costs + extra_for_this_block;
        let projected_profit = self.total_rewards as i64 - pessimistic_cost as i64;
        let projected_profit_avg = (projected_profit
            + (self.avg_profit * (self.avg_window as i64 - 1)))
            / self.avg_window as i64;

        // P
        let checked_p = projected_profit_avg.checked_div(self.da_p_component);
        let p = -checked_p.unwrap_or(0);

        // D
        let slope = projected_profit_avg - self.avg_profit;
        let checked_d = slope.checked_div(self.da_d_component);
        let d = -checked_d.unwrap_or(0);

        let pd_change = p + d;
        let max_change =
            (self.new_exec_price as f64 * self.max_change_percent as f64 / 100.0) as i64;
        let sign = pd_change.signum();
        let signless_da_change = min(max_change, pd_change.abs());
        let da_change = sign * signless_da_change;

        new_da_gas_price = new_da_gas_price + da_change;
        let min = 0;
        let new_da_gas_price = max(new_da_gas_price, min);
        self.new_exec_price + new_da_gas_price as u64
    }
}

// TODO: Add contstructor
pub struct AlgorithmUpdaterV1 {
    pub new_exec_price: u64,
    pub last_da_price: u64,
    pub exec_gas_price_increase_amount: u64,
    pub max_change_percent: u8,

    // L2
    pub l2_block_height: u32,
    pub l2_block_fullness_threshold_percent: u64,
    pub total_da_rewards: u64,

    // DA
    pub da_recorded_block_height: u32,
    pub latest_known_total_da_cost: u64,
    pub projected_total_da_cost: u64,
    pub da_p_component: i64,
    pub da_d_component: i64,
    pub profit_avg: i64,
    pub avg_window: u32,

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
        fullness: (u64, u64),
        block_bytes: u64,
        gas_price: u64,
    ) -> Result<(), Error> {
        let expected = self.l2_block_height.saturating_add(1);
        if height != expected {
            return Err(Error::SkippedL2Block {
                expected,
                got: height,
            })
        } else {
            self.l2_block_height = height;
            let last_profit =
                self.total_da_rewards as i64 - self.projected_total_da_cost as i64;
            self.update_profit_avg(last_profit);
            self.projected_total_da_cost += block_bytes * self.latest_da_cost_per_byte;
            // implicitly deduce what our da gas price was for the l2 block
            self.last_da_price = gas_price.saturating_sub(self.new_exec_price);
            self.update_exec_gas_price(fullness.0, fullness.1);
            let da_reward = fullness.0 * self.last_da_price;
            self.total_da_rewards += da_reward;
            Ok(())
        }
    }

    fn update_profit_avg(&mut self, new_profit: i64) {
        let old_avg = self.profit_avg;
        let new_avg = (old_avg * (self.avg_window as i64 - 1) + new_profit)
            / self.avg_window as i64;
        self.profit_avg = new_avg;
    }

    fn update_exec_gas_price(&mut self, used: u64, capacity: u64) {
        let mut exec_gas_price = self.new_exec_price;
        let fullness_percent = used * 100 / capacity;
        // EXEC PORTION
        if fullness_percent > self.l2_block_fullness_threshold_percent {
            exec_gas_price =
                exec_gas_price.saturating_add(self.exec_gas_price_increase_amount);
        } else if fullness_percent < self.l2_block_fullness_threshold_percent {
            exec_gas_price =
                exec_gas_price.saturating_sub(self.exec_gas_price_increase_amount);
        }
        self.new_exec_price = exec_gas_price;
    }

    fn da_block_update(
        &mut self,
        height: u32,
        block_bytes: u64,
        block_cost: u64,
    ) -> Result<(), Error> {
        let expected = self.da_recorded_block_height.saturating_add(1);
        if height != expected {
            return Err(Error::SkippedDABlock {
                expected: self.da_recorded_block_height.saturating_add(1),
                got: height,
            })
        } else {
            let new_cost_per_byte = block_cost.checked_div(block_bytes).ok_or(
                Error::CouldNotCalculateCostPerByte {
                    bytes: block_bytes,
                    cost: block_cost,
                },
            )?;
            self.da_recorded_block_height = height;
            self.latest_known_total_da_cost += block_cost;
            self.latest_da_cost_per_byte = new_cost_per_byte;
            Ok(())
        }
    }

    fn recalculate_projected_cost(&mut self) {
        // remove all blocks that have been recorded
        self.unrecorded_blocks
            .retain(|block| block.height > self.da_recorded_block_height);
        // add the cost of the remaining blocks
        let projection_portion: u64 = self
            .unrecorded_blocks
            .iter()
            .map(|block| block.block_bytes * self.latest_da_cost_per_byte)
            .sum();
        self.projected_total_da_cost =
            self.latest_known_total_da_cost + projection_portion;
    }

    pub fn algorithm(&self) -> AlgorithmV1 {
        AlgorithmV1 {
            new_exec_price: self.new_exec_price,
            last_da_price: self.last_da_price,
            max_change_percent: self.max_change_percent,

            latest_da_cost_per_byte: self.latest_da_cost_per_byte,
            total_rewards: self.total_da_rewards,
            total_costs: self.projected_total_da_cost,
            avg_profit: self.profit_avg,
            da_p_component: self.da_p_component,
            da_d_component: self.da_d_component,
            avg_window: self.avg_window,
        }
    }
}
