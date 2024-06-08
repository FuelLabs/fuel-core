use plotters::prelude::*;
use rand::{
    rngs::StdRng,
    Rng,
    SeedableRng,
};

use plotters::coord::Shift;

use gas_price_algorithm::{AlgorithmUpdaterV1, AlgorithmV0};

fn gen_noisy_signal(input: f64, components: &[f64]) -> f64 {
    components
        .iter()
        .fold(0f64, |acc, &c| acc + f64::sin(input / c))
        / components.len() as f64
}

fn noisy_cost<T: TryInto<f64>>(input: T) -> f64
where
    <T as TryInto<f64>>::Error: core::fmt::Debug,
{
    const COMPONENTS: &[f64] = &[50.0, 100.0, 300.0, 1000.0, 500.0];
    let input = input.try_into().unwrap();
    gen_noisy_signal(input, COMPONENTS)
}

fn noisy_bytes<T: TryInto<f64>>(input: T) -> f64
where
    <T as TryInto<f64>>::Error: core::fmt::Debug,
{
    const COMPONENTS: &[f64] = &[50.0, 100.0, 300.0, 1000.0, 500.0];
    let input = input.try_into().unwrap();
    gen_noisy_signal(input, COMPONENTS)
}

fn noisy_eth_price<T: TryInto<f64>>(input: T) -> f64
where
    <T as TryInto<f64>>::Error: core::fmt::Debug,
{
    const COMPONENTS: &[f64] = &[70.0, 130.0];
    let input = input.try_into().unwrap();
    gen_noisy_signal(input, COMPONENTS)
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

fn arb_eth_per_bytes_signal(size: u32) -> Vec<u64> {
    (0u32..size)
        .map(noisy_bytes)
        .map(|x| x * 10. + 20.)
        .map(|x| x as u64)
        .collect()
}

fn noisy_fullness<T: TryInto<f64>>(input: T) -> f64
where
    <T as TryInto<f64>>::Error: core::fmt::Debug,
{
    const COMPONENTS: &[f64] = &[-30.0, 40.0, 700.0, -340.0, 400.0];
    let input = input.try_into().unwrap();
    gen_noisy_signal(input, COMPONENTS)
}

fn arb_fullness_signal(size: u32, capacity: u64) -> Vec<(u64, u64)> {
    let mut rng = StdRng::seed_from_u64(888);
    (0u32..size)
        .map(noisy_fullness)
        .map(|x| (0.5 * x + 0.4) * capacity as f64)
        .map(|x| {
            let val = rng.gen_range(-0.25 * capacity as f64..0.25 * capacity as f64);
            x + val
        })
        .map(|x| f64::min(x, capacity as f64))
        .map(|x| f64::max(x, 5.0))
        .map(|x| (x as u64, capacity))
        .collect()
}

fn fullness_and_bytes_per_block(size: usize, capacity: u64) -> Vec<(u64, u64)> {
    let mut rng = StdRng::seed_from_u64(888);
    (0usize..size)
        .map(noisy_fullness)
        .map(|signal| (0.5 * signal + 0.4) * capacity as f64)
        .map(|fullness| {
            // Add noisy to the fullness
            let val = rng.gen_range(-0.25 * capacity as f64..0.25 * capacity as f64);
            fullness + val
        })
        .map(|x| f64::min(x, capacity as f64))
        .map(|x| f64::max(x, 5.0))
        .map(|fullness| {
            // calculate bytes that are an arbitrary fraction of the gas scaled
            let scale = rng.gen_range(0.1..1.0) * 4.0;
            let bytes = fullness * scale;
            (fullness, bytes)
        })
        .map(|(fullness, bytes)| (fullness as u64, bytes as u64))
        .collect()
}

fn arbitrary_da_values() -> Vec<(i64, i64, u8)> {
    let mut rng = StdRng::seed_from_u64(777);
    (0..10_000)
        .map(|_| {
            let p = rng.gen_range(-20_000..20_000);
            let d = rng.gen_range(-500..500);
            // let max_change = rng.gen_range(1..255);
            let max_change = 50;
            (p, d, max_change)
        })
        .filter(|(p, d, _)| *p != 0 && *d != 0)
        .collect()
}

fn main() {
    // Try to find a good p and d value
    let (vals, results, _error) = arbitrary_da_values()
        .into_iter()
        .map(|(p, d, max_change)| {
            let results = run_simulation_v0(p, d, max_change);
            let total_profits = &results.total_profits;

            let total_profits_abs: Vec<_> =
                total_profits.iter().map(|x| x.abs()).collect();
            let sum = total_profits_abs.iter().fold(0i64, |acc, val| {
                let sum = acc.saturating_add(*val);
                sum
            }) as f64;
            let len = total_profits_abs.len() as f64;
            let average_profit_error = sum / len;
            ((p, d, max_change), results, average_profit_error)
        })
        .fold(None, |acc, (vals, result, err)| {
            if let Some((_, _, best_err)) = acc {
                if err < best_err {
                    Some((vals, result, err))
                } else {
                    acc
                }
            } else {
                Some((vals, result, err))
            }
        })
        .unwrap();

    println!("Best p and d values: {:?}", vals);

    let SimulationResults {
        da_recording_cost,
        exec_fullness,
        da_gas_prices,
        exec_gas_prices,
        total_gas_prices,
        total_profits,
        da_rewards,
    } = results;

    // Plotting code starts here
    let plot_width = 640 * 2;
    let plot_height = 480 * 3;

    const FILE_PATH: &str = "gas_prices.png";

    let root =
        BitMapBackend::new(FILE_PATH, (plot_width, plot_height)).into_drawing_area();
    root.fill(&WHITE).unwrap();
    let (upper, lower) = root.split_vertically(plot_height / 3);
    let (middle, bottom) = lower.split_vertically(plot_height / 3);

    draw_da_chart(
        &upper,
        &total_profits,
        &da_recording_cost,
        &da_rewards,
        &da_gas_prices,
        vals.0,
        vals.1,
        vals.2,
    );
    draw_exec_chart(&middle, &total_profits, &exec_fullness, &exec_gas_prices);
    draw_total_gas_price(&bottom, &da_gas_prices, &exec_gas_prices, &total_gas_prices);

    root.present().unwrap();
}

struct SimulationResults {
    da_recording_cost: Vec<u64>,
    exec_fullness: Vec<(u64, u64)>,
    da_gas_prices: Vec<i64>,
    exec_gas_prices: Vec<i64>,
    total_gas_prices: Vec<i64>,
    total_profits: Vec<i64>,
    da_rewards: Vec<u64>,
}

struct SimulationResultsV1 {

}

fn run_simulation_v0(
    p_value_factor: i64,
    d_value_factor: i64,
    max_change_percent: u8,
) -> SimulationResults {
    let min_da_price = 10;
    let min_exec_price = 10;
    let moving_average_window = 10;
    // // TODO: This value is large because it only changes once per `da_record_frequency` blocks.
    // //     Is it possible to decrease if we get the p and d values tuned better? We should be able
    // //     to solve for the lower granularity still, looking at how much we overshoot.
    // let max_change_percent = 200;
    let exec_change_amount = 10;
    let algo = AlgorithmV0::new(
        max_change_percent,
        min_da_price,
        p_value_factor,
        d_value_factor,
        moving_average_window,
        min_exec_price,
        exec_change_amount,
    );

    let capacity = 400;
    let simulation_size = 1_000;

    // Run simulation
    let da_recording_cost = arb_cost_signal(simulation_size);
    let exec_fullness = arb_fullness_signal(simulation_size, capacity);
    let mut da_gas_price: u64 = 100;
    let mut da_gas_prices = vec![da_gas_price as i64];
    let mut exec_gas_price = 0;
    let mut exec_gas_prices = vec![exec_gas_price as i64];
    let mut total_gas_prices = vec![(da_gas_price + exec_gas_price) as i64];
    let mut total_profits = vec![0i64];
    let mut da_rewards = vec![];
    let mut total_da_cost = 0;
    let mut total_da_reward: u64 = 0;

    let da_record_frequency = 12;
    let mut da_record_counter = 0;

    for (da_cost, (used, capacity)) in da_recording_cost.iter().zip(exec_fullness.iter())
    {
        total_da_cost += da_cost;
        let da_reward = da_gas_price.saturating_mul(*used);
        da_rewards.push(da_reward);
        total_da_reward = total_da_reward.saturating_add(da_reward);
        let total_profit = total_da_reward as i64 - total_da_cost as i64;
        total_profits.push(total_profit);
        exec_gas_price = algo.calculate_exec_gas_price(exec_gas_price, *used, *capacity);
        // Only update the da gas price every da_record_frequency blocks
        if da_record_counter % da_record_frequency == 0 {
            da_gas_price =
                algo.calculate_da_gas_price(da_gas_price, total_da_reward, total_da_cost);
        }
        da_record_counter += 1;

        da_gas_prices.push(da_gas_price as i64);
        exec_gas_prices.push(exec_gas_price as i64);
        total_gas_prices.push((da_gas_price + exec_gas_price) as i64);
    }

    SimulationResults {
        da_recording_cost,
        exec_fullness,
        da_gas_prices,
        exec_gas_prices,
        total_gas_prices,
        total_profits,
        da_rewards,
    }
}

fn run_simulation_v1() -> SimulationResults {
    let builder = AlgorithmUpdaterV1::new();
}

fn draw_total_gas_price<DB: DrawingBackend>(
    drawing_area: &DrawingArea<DB, Shift>,
    da_gas_prices: &Vec<i64>,
    exec_gas_prices: &Vec<i64>,
    total_gas_prices: &Vec<i64>,
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
        .unwrap();

    total_gas_price_chart
        .configure_mesh()
        .y_desc("Gas Price")
        .x_desc("Block")
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
    total_profits: &Vec<i64>,
    exec_fullness: &Vec<(u64, u64)>,
    exec_gas_prices: &Vec<i64>,
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
        .y_desc("Fullness Percentage")
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
    total_profits: &Vec<i64>,
    da_recording_cost: &Vec<u64>,
    da_rewards: &Vec<u64>,
    da_gas_prices: &Vec<i64>,
    p_value: i64,
    d_value: i64,
    max_change: u8,
) {
    let da_min = *total_profits.iter().min().unwrap() - 10_000;
    let da_max = *total_profits.iter().max().unwrap() as i64 + 10_000;

    let mut da_chart = ChartBuilder::on(drawing_area)
        .caption(
            &format!("DA Recording Costs (p: {p_value:?}, d: {d_value:?}, max_change: {max_change:?}%)"),
            ("sans-serif", 50).into_font(),
        )
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
                .map(|(x, y)| (x, *y as i64)),
            &COST_COLOR,
        ))
        .unwrap()
        .label("Cost")
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], &COST_COLOR));

    da_chart
        .draw_series(LineSeries::new(
            da_rewards.iter().enumerate().map(|(x, y)| (x, *y as i64)),
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
