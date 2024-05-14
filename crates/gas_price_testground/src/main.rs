use plotters::prelude::*;
use rand::{
    rngs::StdRng,
    Rng,
    SeedableRng,
};
use rand_distr::{
    Distribution,
    Normal,
};
use std::cmp::max;

struct Algorithm {
    amount: u64,
    min: u64,
    p_value_factor: u64,
}

impl Algorithm {
    fn calculate_gas_price(
        &self,
        old_gas_price: u64,
        total_production_reward: u64,
        total_da_recording_cost: u64,
        used: u64,
        capacity: u64,
    ) -> u64 {
        if total_da_recording_cost > total_production_reward {
            let add =
                (total_da_recording_cost - total_production_reward) / self.p_value_factor;
            old_gas_price + add
        } else {
            if used > capacity / 2 {
                todo!();
            } else if total_production_reward > total_da_recording_cost {
                let sub = (total_production_reward - total_da_recording_cost)
                    / self.p_value_factor;
                max(old_gas_price.saturating_sub(sub), self.min)
            } else {
                old_gas_price
            }
        }
    }
}

fn noisy<T: TryInto<f64>>(input: T) -> f64
where
    <T as TryInto<f64>>::Error: core::fmt::Debug,
{
    let input = input.try_into().unwrap();
    let components: &[f64] = &[50.0, 100.0, 300.0, 1000.0, 500.0];
    components
        .iter()
        .fold(0f64, |acc, &c| acc + f64::sin(input / c))
}

fn arb_cost_signal() -> Vec<u64> {
    let mut rng = StdRng::seed_from_u64(999);
    (0u32..1_000)
        .map(noisy)
        .map(|x| {
            let val = rng.gen_range(-0.5..0.5);
            x + val
        })
        .map(|x| x * 10_000. + 20_000.)
        .map(|x| x as u64)
        .collect()
}

fn main() {
    let amount = 1;
    let min = 10;
    let p_value_factor = 300;
    let algo = Algorithm {
        amount,
        min,
        p_value_factor,
    };

    let gas_spent = 200;
    let simulation_size = 1_000;

    // Run simulation
    let da_recording_cost = arb_cost_signal();
    let mut gas_price = 100;
    let mut gas_prices = vec![gas_price as i32];
    let mut total_profits = vec![0i32];
    let mut rewards = vec![];
    let mut total_cost = 0;
    let mut total_reward = 0;
    // 50% capacity
    let used = gas_spent;
    let capacity = gas_spent * 2;
    for cost in &da_recording_cost {
        total_cost += cost;
        let reward = gas_price * gas_spent;
        rewards.push(reward);
        total_reward += reward;
        let total_profit = total_reward as i32 - total_cost as i32;
        total_profits.push(total_profit);
        gas_price =
            algo.calculate_gas_price(gas_price, total_reward, total_cost, used, capacity);
        gas_prices.push(gas_price as i32);
    }

    // Plotting code starts here
    let root = BitMapBackend::new("gas_prices.png", (640, 480)).into_drawing_area();
    root.fill(&WHITE).unwrap();

    let min = *total_profits.iter().min().unwrap() - 10_000;
    let max = *rewards.iter().max().unwrap() as i32 + 10_000;
    // let min = -10_000;
    // let max = 40_000;

    let mut chart = ChartBuilder::on(&root)
        .caption("Gas Prices Over Time", ("sans-serif", 50).into_font())
        .margin(5)
        .x_label_area_size(40)
        .y_label_area_size(60)
        .right_y_label_area_size(40)
        .build_cartesian_2d(0..total_profits.len(), min..max)
        .unwrap()
        .set_secondary_coord(0..gas_prices.len(), 0..*gas_prices.iter().max().unwrap());

    chart
        .configure_mesh()
        .y_desc("Profit")
        .x_desc("Block")
        .draw()
        .unwrap();

    chart
        .configure_secondary_axes()
        .y_desc("Gas Price")
        .draw()
        .unwrap();

    const PRICE_COLOR: RGBColor = BLACK;
    const PROFIT_COLOR: RGBColor = BLUE;
    const REWARD_COLOR: RGBColor = GREEN;
    const COST_COLOR: RGBColor = RED;

    chart
        .draw_series(LineSeries::new(
            total_profits.iter().enumerate().map(|(x, y)| (x, *y)),
            &PROFIT_COLOR,
        ))
        .unwrap()
        .label("Profit")
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], &PROFIT_COLOR));

    chart
        .draw_series(LineSeries::new(
            da_recording_cost
                .into_iter()
                .enumerate()
                .map(|(x, y)| (x, y as i32)),
            &COST_COLOR,
        ))
        .unwrap()
        .label("Cost")
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], &COST_COLOR));

    chart
        .draw_series(LineSeries::new(
            rewards.iter().enumerate().map(|(x, y)| (x, *y as i32)),
            &REWARD_COLOR,
        ))
        .unwrap()
        .label("Reward")
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], &REWARD_COLOR));

    chart
        .draw_secondary_series(LineSeries::new(
            gas_prices.iter().enumerate().map(|(x, y)| (x, *y)),
            &PRICE_COLOR,
        ))
        .unwrap()
        .label("Gas Price")
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], &PRICE_COLOR));

    chart
        .configure_series_labels()
        .background_style(&WHITE.mix(0.8))
        .border_style(&BLACK)
        .draw()
        .unwrap();

    root.present().unwrap();
}
