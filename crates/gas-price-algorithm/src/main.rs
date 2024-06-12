use plotters::prelude::*;
use rand::{
    rngs::StdRng,
    Rng,
    SeedableRng,
};

use plotters::coord::Shift;

use gas_price_algorithm::{
    AlgorithmUpdaterV1,
    AlgorithmV0,
    RecordedBlock,
};

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

fn fullness_and_bytes_per_block(size: usize, capacity: u64) -> Vec<(u64, u64)> {
    let mut rng = StdRng::seed_from_u64(888);

    let fullness_noise: Vec<_> = std::iter::repeat(())
        .take(size)
        .map(|_| rng.gen_range(-0.25..0.25))
        .map(|val| val * capacity as f64)
        .collect();

    let bytes_scale: Vec<_> = std::iter::repeat(())
        .take(size)
        .map(|_| rng.gen_range(0.5..1.0))
        .map(|x| x * 4.0)
        .collect();

    (0usize..size)
        .map(|val| val as f64)
        .map(noisy_fullness)
        .map(|signal| (0.5 * signal + 0.5) * capacity as f64)
        .zip(fullness_noise)
        .map(|(fullness, noise)| fullness + noise)
        .map(|x| f64::min(x, capacity as f64))
        .map(|x| f64::max(x, 5.0))
        .zip(bytes_scale)
        .map(|(fullness, bytes_scale)| {
            let bytes = fullness * bytes_scale;
            (fullness, bytes)
        })
        .map(|(fullness, bytes)| (fullness as u64, bytes as u64))
        .collect()
}

fn arb_cost_per_byte(size: u32) -> Vec<u64> {
    (0u32..size)
        .map(noisy_eth_price)
        .map(|x| x * 100. + 110.)
        .map(|x| x as u64)
        .collect()
}

fn main() {
    // simulation parameters
    let fullness_threshold = 50;
    let exec_gas_price_increase_amount = 10;
    let da_p_component = 10_000;
    let da_d_component = 10_000;
    let starting_gas_per_byte = 100;
    let size = 100;
    let da_recording_rate = 10;
    let capacity = 20_000;
    let fullness_and_bytes = fullness_and_bytes_per_block(size, capacity);
    let da_cost_per_byte = arb_cost_per_byte(size as u32);

    let l2_blocks = fullness_and_bytes
        .iter()
        .map(|(fullness, bytes)| (*fullness, *bytes))
        .collect::<Vec<_>>();
    let da_blocks = fullness_and_bytes
        .iter()
        .zip(da_cost_per_byte.iter())
        .enumerate()
        .fold(
            (vec![], vec![]),
            |(mut delayed, mut recorded), (index, ((fullness, bytes), cost_per_byte))| {
                let total_cost = bytes * cost_per_byte;
                let height = index as u32 + 1;
                let converted = RecordedBlock {
                    height,
                    block_bytes: *bytes,
                    block_cost: total_cost,
                };
                delayed.push(converted);
                if delayed.len() == da_recording_rate {
                    recorded.push(Some(delayed));
                    (vec![], recorded)
                } else {
                    recorded.push(None);
                    (delayed, recorded)
                }
            },
        )
        .1;

    let blocks = l2_blocks.iter().zip(da_blocks.iter()).enumerate();

    let mut updater = AlgorithmUpdaterV1 {
        new_exec_price: 800,
        last_da_price: 800,
        l2_block_height: 0,
        l2_block_fullness_threshold_percent: fullness_threshold,
        exec_gas_price_increase_amount,
        total_da_rewards: 0,
        da_recorded_block_height: 0,
        latest_da_cost_per_byte: starting_gas_per_byte,
        projected_total_da_cost: 0,
        latest_known_total_da_cost: 0,
        unrecorded_blocks: vec![],
        da_p_component,
        da_d_component,
        last_profit: 0,
    };

    let mut gas_prices = vec![];
    let mut exec_gas_prices = vec![];
    let mut da_gas_prices = vec![];
    let mut actual_rewards = vec![];
    let mut projected_costs = vec![];
    let mut actual_costs = vec![];
    let mut pessimistic_costs = vec![];
    let pessimistic_bytes = capacity * 4;
    for (index, (l2_block, da_block)) in blocks {
        let height = index as u32 + 1;
        let gas_price = updater.algorithm().calculate(pessimistic_bytes);
        gas_prices.push(gas_price);
        let (fullness, bytes) = l2_block;
        exec_gas_prices.push(updater.new_exec_price);
        updater
            .update_l2_block_data(height, (*fullness, capacity), *bytes, gas_price)
            .unwrap();
        da_gas_prices.push(updater.last_da_price);
        if let Some(da_blocks) = da_block {
            let mut total_costs = updater.latest_known_total_da_cost;
            for block in da_blocks {
                total_costs += block.block_cost;
                actual_costs.push(total_costs);
            }
            updater.update_da_record_data(da_blocks.to_owned()).unwrap();
        }
        pessimistic_costs.push(pessimistic_bytes * updater.latest_da_cost_per_byte);
        actual_rewards.push(updater.total_da_rewards);
        projected_costs.push(updater.projected_total_da_cost);
        let profit =
            updater.total_da_rewards as i64 - updater.projected_total_da_cost as i64;
        dbg!(profit);
    }

    // Plotting code starts here
    let plot_width = 640 * 2;
    let plot_height = 480 * 3;

    const FILE_PATH: &str = "gas_prices.png";

    let root =
        BitMapBackend::new(FILE_PATH, (plot_width, plot_height)).into_drawing_area();
    root.fill(&WHITE).unwrap();
    let (upper, lower) = root.split_vertically(plot_height / 3);
    let (middle, bottom) = lower.split_vertically(plot_height / 3);

    let fullness = fullness_and_bytes
        .iter()
        .map(|(fullness, _)| (*fullness, capacity))
        .collect();
    draw_fullness(&upper, &fullness, "Fullness");

    let actual_profit: Vec<i64> = actual_costs
        .iter()
        .zip(actual_rewards.iter())
        .map(|(cost, reward)| *reward as i64 - *cost as i64)
        .collect();
    let projected_profit: Vec<i64> = projected_costs
        .iter()
        .zip(actual_rewards.iter())
        .map(|(cost, reward)| *reward as i64 - *cost as i64)
        .collect();
    draw_profit(
        &middle,
        &actual_profit,
        &projected_profit,
        &pessimistic_costs,
        "Profit",
    );
    draw_gas_prices(
        &bottom,
        &gas_prices,
        &exec_gas_prices,
        &da_gas_prices,
        "Gas Prices",
    );

    root.present().unwrap();
}

fn draw_gas_prices(
    drawing_area: &DrawingArea<BitMapBackend, Shift>,
    gas_prices: &Vec<u64>,
    exec_gas_prices: &Vec<u64>,
    da_gas_prices: &Vec<u64>,
    title: &str,
) {
    // let min = *gas_prices.iter().min().unwrap();
    let min = 0;
    let max = *gas_prices.iter().max().unwrap();

    let mut chart = ChartBuilder::on(drawing_area)
        .caption(title, ("sans-serif", 50).into_font())
        .margin(5)
        .x_label_area_size(40)
        .y_label_area_size(60)
        .right_y_label_area_size(40)
        .build_cartesian_2d(0..gas_prices.len(), min..max)
        .unwrap();

    chart
        .configure_mesh()
        .y_desc("Gas Price")
        .x_desc("Block")
        .draw()
        .unwrap();

    chart
        .draw_series(LineSeries::new(
            gas_prices.iter().enumerate().map(|(x, y)| (x, *y)),
            &BLACK,
        ))
        .unwrap()
        .label("Gas Price")
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], &BLACK));

    // Draw the exec gas prices
    chart
        .draw_series(LineSeries::new(
            exec_gas_prices.iter().enumerate().map(|(x, y)| (x, *y)),
            &RED,
        ))
        .unwrap()
        .label("Exec Gas Price")
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], &RED));

    // Draw the da gas prices
    chart
        .draw_series(LineSeries::new(
            da_gas_prices.iter().enumerate().map(|(x, y)| (x, *y)),
            &BLUE,
        ))
        .unwrap()
        .label("DA Gas Price")
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], &BLUE));

    chart
        .configure_series_labels()
        .background_style(&WHITE.mix(0.8))
        .border_style(&BLACK)
        .draw()
        .unwrap();
}

fn draw_fullness(
    drawing_area: &DrawingArea<BitMapBackend, Shift>,
    fullness: &Vec<(u64, u64)>,
    title: &str,
) {
    let min = 0;
    let max = 100;

    let mut chart = ChartBuilder::on(drawing_area)
        .caption(title, ("sans-serif", 50).into_font())
        .margin(5)
        .x_label_area_size(40)
        .y_label_area_size(60)
        .right_y_label_area_size(40)
        .build_cartesian_2d(0..fullness.len(), min..max)
        .unwrap();

    chart
        .configure_mesh()
        .y_desc("Fullness Percentage")
        .x_desc("Block")
        .draw()
        .unwrap();

    chart
        .draw_series(LineSeries::new(
            fullness
                .iter()
                .map(|(x, y)| (*x as f64 / *y as f64) * 100.)
                .map(|x| x as i32)
                .enumerate(),
            &BLACK,
        ))
        .unwrap();

    chart
        .configure_series_labels()
        .background_style(&WHITE.mix(0.8))
        .border_style(&BLACK)
        .draw()
        .unwrap();
}

fn draw_profit(
    drawing_area: &DrawingArea<BitMapBackend, Shift>,
    actual_profit: &Vec<i64>,
    projected_profit: &Vec<i64>,
    pessimistic_block_costs: &Vec<u64>,
    title: &str,
) {
    let min = *actual_profit.iter().min().unwrap();
    let max = *actual_profit.iter().max().unwrap();

    let mut chart = ChartBuilder::on(drawing_area)
        .caption(title, ("sans-serif", 50).into_font())
        .margin(5)
        .x_label_area_size(40)
        .y_label_area_size(60)
        .right_y_label_area_size(40)
        .build_cartesian_2d(0..actual_profit.len(), min..max)
        .unwrap();

    chart
        .configure_mesh()
        .y_desc("Profit")
        .x_desc("Block")
        .draw()
        .unwrap();

    chart
        .draw_series(LineSeries::new(
            actual_profit.iter().enumerate().map(|(x, y)| (x, *y)),
            &BLACK,
        ))
        .unwrap()
        .label("Actual Profit")
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], &BLACK));

    chart
        .draw_series(LineSeries::new(
            projected_profit.iter().enumerate().map(|(x, y)| (x, *y)),
            &RED,
        ))
        .unwrap()
        .label("Projected Profit")
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], &RED));

    // draw the block bytes
    chart
        .draw_series(LineSeries::new(
            pessimistic_block_costs
                .iter()
                .enumerate()
                .map(|(x, y)| (x, *y as i64)),
            &BLUE,
        ))
        .unwrap()
        .label("Pessimistic Block Costs")
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], &BLUE));

    chart
        .configure_series_labels()
        .background_style(&WHITE.mix(0.8))
        .border_style(&BLACK)
        .draw()
        .unwrap();
}
