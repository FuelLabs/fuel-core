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
    da_p_value_factor: i32,
    da_d_value_factor: i32,
    moving_average_profit: RefCell<i32>,
    last_profit: RefCell<i32>,
    profit_slope: RefCell<i32>,
    moving_average_window: i32,
    // EXEC
    exec_change_amount: u64,
    min_exec_price: u64,
}

impl AlgorithmV1 {
    pub fn new(
        da_max_change_percent: u8,
        min_da_price: u64,
        da_p_value_factor: i32,
        da_d_value_factor: i32,
        moving_average_window: i32,
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
        let new_profit = total_production_reward as i32 - total_da_recording_cost as i32;
        self.calculate_new_moving_average(new_profit);
        self.calculate_profit_slope(*self.moving_average_profit.borrow());
        let avg_profit = *self.moving_average_profit.borrow();
        let slope = *self.profit_slope.borrow();

        let max_change =
            (old_da_gas_price * self.da_max_change_percent as u64 / 100) as i32;

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
        let new_da_gas_price = old_da_gas_price as i32 - change;
        max(new_da_gas_price as u64, self.min_da_price)
    }

    pub fn calculate_exec_gas_price(
        &self,
        old_exec_gas_price: u64,
        used: u64,
        capacity: u64,
    ) -> u64 {
        let new = if used > capacity / 2 {
            old_exec_gas_price.saturating_add(self.exec_change_amount)
        } else if used < capacity / 2 {
            old_exec_gas_price.saturating_sub(self.exec_change_amount)
        } else {
            old_exec_gas_price
        };
        max(new, self.min_exec_price)
    }

    fn calculate_new_moving_average(&self, new_profit: i32) {
        let old = *self.moving_average_profit.borrow();
        *self.moving_average_profit.borrow_mut() = (old
            * (self.moving_average_window - 1))
            .saturating_add(new_profit)
            .saturating_div(self.moving_average_window);
    }

    fn calculate_profit_slope(&self, new_profit: i32) {
        let old_profit = *self.last_profit.borrow();
        *self.profit_slope.borrow_mut() = new_profit - old_profit;
        *self.last_profit.borrow_mut() = new_profit;
    }
}
