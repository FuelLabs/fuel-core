use plotters::prelude::*;
use rand::{
    rngs::StdRng,
    Rng,
    SeedableRng,
};
use std::cell::RefCell;

use plotters::coord::Shift;
use std::cmp::{
    max,
    min,
};

struct Algorithm {
    amount: u64,
    max_change_percent: u8,
    min_da_price: u64,
    min_exec_price: u64,
    p_value_factor: i32,
    d_value_factor: i32,
    moving_average_profit: RefCell<i32>,
    last_profit: RefCell<i32>,
    profit_slope: RefCell<i32>,
    moving_average_window: i32,
}

impl Algorithm {
    fn calculate_da_gas_price(
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

        let max_change = (old_da_gas_price * self.max_change_percent as u64 / 100) as i32;

        // if p > 0 and dp/db > 0, decrease
        // if p > 0 and dp/db < 0, hold/moderate
        // if p < 0 and dp/db < 0, increase
        // if p < 0 and dp/db > 0, hold/moderate
        let p_comp = avg_profit / self.p_value_factor;
        let d_comp = slope / self.d_value_factor;
        let pd_change = p_comp + d_comp;
        let change = min(max_change, pd_change.abs());
        let sign = pd_change.signum();
        let change = change * sign;
        let new_da_gas_price = old_da_gas_price as i32 - change;
        max(new_da_gas_price as u64, self.min_da_price)
    }

    fn calculate_exec_gas_price(
        &self,
        old_exec_gas_price: u64,
        used: u64,
        capacity: u64,
    ) -> u64 {
        let new = if used > capacity / 2 {
            old_exec_gas_price.saturating_add(10)
        } else if used < capacity / 2 {
            old_exec_gas_price.saturating_sub(10)
        } else {
            old_exec_gas_price
        };
        max(new, self.min_exec_price)
    }

    fn calculate_gas_price(
        &self,
        old_da_gas_price: u64,
        old_exec_gas_price: u64,
        total_da_production_reward: u64,
        total_da_recording_cost: u64,
        used: u64,
        capacity: u64,
    ) -> (u64, u64) {
        let new_da_gas_price = self.calculate_da_gas_price(
            old_da_gas_price,
            total_da_production_reward,
            total_da_recording_cost,
        );
        let new_exec_gas_price =
            self.calculate_exec_gas_price(old_exec_gas_price, used, capacity);
        (new_da_gas_price, new_exec_gas_price)
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

fn noisy_cost<T: TryInto<f64>>(input: T) -> f64
where
    <T as TryInto<f64>>::Error: core::fmt::Debug,
{
    let input = input.try_into().unwrap();
    let components: &[f64] = &[50.0, 100.0, 300.0, 1000.0, 500.0];
    components
        .iter()
        .fold(0f64, |acc, &c| acc + f64::sin(input / c))
        / components.len() as f64
}

fn arb_cost_signal(size: u32) -> Vec<u64> {
    let mut rng = StdRng::seed_from_u64(999);
    (0u32..size)
        .map(noisy_cost)
        .map(|x| x * 10_000. + 20_000.)
        .map(|x| {
            let val = rng.gen_range(-19_000.0..10_000.0);
            x + val
        })
        .map(|x| x as u64)
        .collect()
}

fn noisy_fullness<T: TryInto<f64>>(input: T) -> f64
where
    <T as TryInto<f64>>::Error: core::fmt::Debug,
{
    let input = input.try_into().unwrap();
    let components: &[f64] = &[30.0, 40.0, 700.0, 340.0, 400.0];
    components
        .iter()
        .fold(0f64, |acc, &c| acc + f64::sin(input / c))
        / components.len() as f64
}

fn arb_fullness_signal(size: u32, capacity: u64) -> Vec<(u64, u64)> {
    let mut rng = StdRng::seed_from_u64(888);
    (0u32..size)
        .map(noisy_fullness)
        .map(|x| (0.5 * x + 0.5) * capacity as f64)
        .map(|x| {
            let val = rng.gen_range(-0.5 * capacity as f64..0.5 * capacity as f64);
            x + val
        })
        .map(|x| f64::min(x, capacity as f64))
        .map(|x| (x as u64, capacity))
        .collect()
}

fn main() {
    let amount = 1;
    let min = 10;
    let p_value_factor = 4_000;
    let d_value_factor = 100;
    let moving_average_window = 10;
    let max_change_percent = 15;
    let algo = Algorithm {
        amount,
        max_change_percent,
        min_da_price: min,
        p_value_factor,
        d_value_factor,
        moving_average_profit: RefCell::new(0),
        last_profit: RefCell::new(0),
        profit_slope: RefCell::new(0),
        moving_average_window,
    };

    let gas_spent = 200;
    let simulation_size = 100;

    // Run simulation
    let da_recording_cost = arb_cost_signal(simulation_size);
    let exec_fullness = arb_fullness_signal(simulation_size, gas_spent * 2);
    let mut da_gas_price = 100;
    let mut da_gas_prices = vec![da_gas_price as i32];
    let mut exec_gas_price = 0;
    let mut exec_gas_prices = vec![exec_gas_price as i32];
    let mut total_gas_prices = vec![(da_gas_price + exec_gas_price) as i32];
    let mut total_profits = vec![0i32];
    let mut da_rewards = vec![];
    let mut total_da_cost = 0;
    let mut total_da_reward = 0;
    // 50% capacity
    for (da_cost, (used, capacity)) in da_recording_cost.iter().zip(exec_fullness.iter())
    {
        total_da_cost += da_cost;
        let da_reward = da_gas_price * used;
        // let exec_reward = exec_gas_price * gas_spent;
        da_rewards.push(da_reward);
        total_da_reward += da_reward;
        let total_profit = total_da_reward as i32 - total_da_cost as i32;
        total_profits.push(total_profit);
        (da_gas_price, exec_gas_price) = algo.calculate_gas_price(
            da_gas_price,
            exec_gas_price,
            total_da_reward,
            total_da_cost,
            *used,
            *capacity,
        );
        da_gas_prices.push(da_gas_price as i32);
        exec_gas_prices.push(exec_gas_price as i32);
        total_gas_prices.push((da_gas_price + exec_gas_price) as i32);
    }
    let plot_width = 640 * 2;
    let plot_height = 480 * 3;

    // Plotting code starts here
    let root = BitMapBackend::new("gas_prices.png", (plot_width, plot_height))
        .into_drawing_area();
    root.fill(&WHITE).unwrap();
    let (upper, lower) = root.split_vertically(plot_height / 3);
    let (middle, bottom) = lower.split_vertically(plot_height / 3);

    draw_da_chart(
        &upper,
        &total_profits,
        &da_recording_cost,
        &da_rewards,
        &da_gas_prices,
    );

    draw_exec_chart(&middle, &total_profits, &exec_fullness, &exec_gas_prices);

    draw_total_gas_price(&bottom, &da_gas_prices, &exec_gas_prices, &total_gas_prices);

    root.present().unwrap();
}

fn draw_total_gas_price<DB: DrawingBackend>(
    drawing_area: &DrawingArea<DB, Shift>,
    da_gas_prices: &Vec<i32>,
    exec_gas_prices: &Vec<i32>,
    total_gas_prices: &Vec<i32>,
) {
    const DA_COLOR: RGBColor = BLUE;
    const EXEC_COLOR: RGBColor = RED;
    const TOTAL_COLOR: RGBColor = BLACK;

    let min = 0;
    let max = *total_gas_prices.iter().max().unwrap();

    let mut total_gas_price_chart = ChartBuilder::on(drawing_area)
        .caption("Total Gas Price", ("sans-serif", 50).into_font())
        .margin(5)
        .x_label_area_size(40)
        .y_label_area_size(60)
        .right_y_label_area_size(40)
        .build_cartesian_2d(0..total_gas_prices.len(), min..max)
        .unwrap()
        .set_secondary_coord(
            0..da_gas_prices.len(),
            0..*da_gas_prices.iter().max().unwrap() + 9,
        );

    total_gas_price_chart
        .configure_mesh()
        .y_desc("Gas Price")
        .x_desc("Block")
        .draw()
        .unwrap();

    total_gas_price_chart
        .configure_secondary_axes()
        .y_desc("Gas Price")
        .draw()
        .unwrap();

    total_gas_price_chart
        .draw_series(LineSeries::new(
            da_gas_prices.iter().enumerate().map(|(x, y)| (x, *y)),
            &DA_COLOR,
        ))
        .unwrap()
        .label("DA Gas Price")
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], &DA_COLOR));

    total_gas_price_chart
        .draw_series(LineSeries::new(
            exec_gas_prices.iter().enumerate().map(|(x, y)| (x, *y)),
            &EXEC_COLOR,
        ))
        .unwrap()
        .label("Exec Gas Price")
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], &EXEC_COLOR));

    total_gas_price_chart
        .draw_series(LineSeries::new(
            total_gas_prices.iter().enumerate().map(|(x, y)| (x, *y)),
            &TOTAL_COLOR,
        ))
        .unwrap()
        .label("Total Gas Price")
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], &TOTAL_COLOR));

    total_gas_price_chart
        .configure_series_labels()
        .background_style(&WHITE.mix(0.8))
        .border_style(&BLACK)
        .draw()
        .unwrap();
}

fn draw_exec_chart<DB: DrawingBackend>(
    drawing_area: &DrawingArea<DB, Shift>,
    total_profits: &Vec<i32>,
    exec_fullness: &Vec<(u64, u64)>,
    exec_gas_prices: &Vec<i32>,
) {
    const PRICE_COLOR: RGBColor = BLACK;
    const FULLNESS_COLOR: RGBColor = RED;

    let exec_min = 0;
    let exec_max = 100;

    let mut exec_chart = ChartBuilder::on(drawing_area)
        .caption("Execution", ("sans-serif", 50).into_font())
        .margin(5)
        .x_label_area_size(40)
        .y_label_area_size(60)
        .right_y_label_area_size(40)
        .build_cartesian_2d(0..total_profits.len(), exec_min..exec_max)
        .unwrap()
        .set_secondary_coord(
            0..exec_gas_prices.len(),
            0..*exec_gas_prices.iter().max().unwrap() + 9,
        );

    exec_chart
        .configure_mesh()
        .y_desc("Fullness")
        .x_desc("Block")
        .draw()
        .unwrap();

    exec_chart
        .configure_secondary_axes()
        .y_desc("Gas Price")
        .draw()
        .unwrap();

    exec_chart
        .draw_series(LineSeries::new(
            exec_fullness
                .iter()
                .map(|(x, y)| (*x as f64 / *y as f64) * 100.)
                .map(|x| x as i32)
                .enumerate(),
            &FULLNESS_COLOR,
        ))
        .unwrap()
        .label("Fullness Percentage")
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], &FULLNESS_COLOR));

    exec_chart
        .draw_secondary_series(LineSeries::new(
            exec_gas_prices.iter().enumerate().map(|(x, y)| (x, *y)),
            &PRICE_COLOR,
        ))
        .unwrap()
        .label("Gas Price")
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], &PRICE_COLOR));

    exec_chart
        .configure_series_labels()
        .background_style(&WHITE.mix(0.8))
        .border_style(&BLACK)
        .draw()
        .unwrap();
}

fn draw_da_chart<DB: DrawingBackend>(
    drawing_area: &DrawingArea<DB, Shift>,
    total_profits: &Vec<i32>,
    da_recording_cost: &Vec<u64>,
    da_rewards: &Vec<u64>,
    da_gas_prices: &Vec<i32>,
) {
    let da_min = *total_profits.iter().min().unwrap() - 10_000;
    let da_max = *da_rewards.iter().max().unwrap() as i32 + 10_000;

    let mut da_chart = ChartBuilder::on(drawing_area)
        .caption("DA Recording Costs", ("sans-serif", 50).into_font())
        .margin(5)
        .x_label_area_size(40)
        .y_label_area_size(60)
        .right_y_label_area_size(40)
        .build_cartesian_2d(0..total_profits.len(), da_min..da_max)
        .unwrap()
        .set_secondary_coord(
            0..da_gas_prices.len(),
            0..*da_gas_prices.iter().max().unwrap(),
        );

    da_chart
        .configure_mesh()
        .y_desc("Profit/Cost/Reward")
        .x_desc("Block")
        .draw()
        .unwrap();

    da_chart
        .configure_secondary_axes()
        .y_desc("Gas Price")
        .draw()
        .unwrap();

    const PRICE_COLOR: RGBColor = BLACK;
    const PROFIT_COLOR: RGBColor = BLUE;
    const REWARD_COLOR: RGBColor = GREEN;
    const COST_COLOR: RGBColor = RED;

    da_chart
        .draw_series(LineSeries::new(
            total_profits.iter().enumerate().map(|(x, y)| (x, *y)),
            &PROFIT_COLOR,
        ))
        .unwrap()
        .label("Profit")
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], &PROFIT_COLOR));

    da_chart
        .draw_series(LineSeries::new(
            da_recording_cost
                .into_iter()
                .enumerate()
                .map(|(x, y)| (x, *y as i32)),
            &COST_COLOR,
        ))
        .unwrap()
        .label("Cost")
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], &COST_COLOR));

    da_chart
        .draw_series(LineSeries::new(
            da_rewards.iter().enumerate().map(|(x, y)| (x, *y as i32)),
            &REWARD_COLOR,
        ))
        .unwrap()
        .label("Reward")
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], &REWARD_COLOR));

    da_chart
        .draw_secondary_series(LineSeries::new(
            da_gas_prices.iter().enumerate().map(|(x, y)| (x, *y)),
            &PRICE_COLOR,
        ))
        .unwrap()
        .label("Gas Price")
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], &PRICE_COLOR));

    da_chart
        .configure_series_labels()
        .background_style(&WHITE.mix(0.8))
        .border_style(&BLACK)
        .draw()
        .unwrap();
}
