use std::{
    cell::RefCell,
    cmp::{
        max,
        min,
    },
};

pub struct AlgorithmV1 {
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

pub struct AlgorithmBuilderV1 {
    total_reward: u64,
    actual_total_cost: u64,
    projected_total_cost: u64,
    latest_gas_per_byte: u64,
}

impl AlgorithmBuilderV1 {
    pub fn new() -> Self {
        Self {
            total_reward: 0,
            actual_total_cost: 0,
            projected_total_cost: 0,
        }
    }

    pub fn update_reward(&mut self, reward: u64) -> &mut Self {
        self.total_reward += reward;
        self
    }

    pub fn update_projected_cost(&mut self, cost: u64) -> &mut Self {
        self.projected_total_cost += cost;
        self
    }
    pub fn update_actual_cost(
        &mut self,
        cost: u64,
        latest_gas_per_byte: u64,
    ) -> &mut Self {
        self.actual_total_cost += cost;
        self.projected_total_cost = self.actual_total_cost;
        self.latest_gas_per_byte = latest_gas_per_byte;
        self
    }

    pub fn build(self) -> AlgorithmV1 {
        todo!()
    }
}

impl AlgorithmV1 {
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

    pub fn calculate_gas_price(block_bytes: u64) -> u64 {
        todo!()
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
