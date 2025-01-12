use crate::simulation::SimulationResults;
use plotters::{
    coord::Shift,
    prelude::*,
};

const WEI_PER_ETH: f64 = 1_000_000_000_000_000_000.0;
pub fn display_results(results: SimulationResults) -> anyhow::Result<()> {
    graph_results(&results)?;
    print_details(&results)?;

    Ok(())
}

fn print_details(results: &SimulationResults) -> anyhow::Result<()> {
    let profits_eth = results
        .profits
        .iter()
        .map(|profit| *profit as f64 / WEI_PER_ETH)
        .collect::<Vec<_>>();
    let max_gas_price = results.gas_price.iter().max();
    let min_gas_price = results.gas_price.iter().min();
    let max_profit_eth = profits_eth.iter().max_by(|a, b| a.partial_cmp(b).unwrap());
    let min_profit_eth = profits_eth.iter().min_by(|a, b| a.partial_cmp(b).unwrap());
    let final_profit = profits_eth.last().unwrap();
    let final_cost = results
        .costs
        .last()
        .map(|x| *x as f64 / WEI_PER_ETH)
        .unwrap();
    let final_reward = results
        .rewards
        .last()
        .map(|x| *x as f64 / WEI_PER_ETH)
        .unwrap();
    let sample_size = results.profits.len();
    const DAYS_IN_SECONDS: usize = 24 * 60 * 60;
    const HOURS_IN_SECONDS: usize = 60 * 60;
    const MINUTES_IN_SECONDS: usize = 60;
    let sample_length_days = sample_size / DAYS_IN_SECONDS;
    let sample_length_hours = (sample_size % DAYS_IN_SECONDS) / HOURS_IN_SECONDS;
    let sample_length_minutes = (sample_size % HOURS_IN_SECONDS) / MINUTES_IN_SECONDS;
    let sample_length_seconds = sample_size % MINUTES_IN_SECONDS;

    println!(
        "Sample size: {:?} blocks ({:?} days, {:?} hours, {:?} minutes, {:?} seconds)",
        sample_size,
        sample_length_days,
        sample_length_hours,
        sample_length_minutes,
        sample_length_seconds
    );

    println!("Max gas price: {:?}", max_gas_price);
    println!("Min gas price: {:?}", min_gas_price);
    println!("Final gas price: {:?}", results.gas_price.last().unwrap());

    println!("Max profit ETH: {:?}", max_profit_eth);
    println!("Min profit ETH: {:?}", min_profit_eth);
    println!("Final profit: {:?}", final_profit);

    println!(
        "Max cost_per_byte: {:?}",
        results.cost_per_byte.iter().max()
    );

    println!("Final cost: {:?}", final_cost);

    println!("Final reward: {:?}", final_reward);

    Ok(())
}

fn graph_results(results: &SimulationResults) -> anyhow::Result<()> {
    let root_area =
        BitMapBackend::new("charts/results.png", (1280, 720)).into_drawing_area();
    root_area.fill(&WHITE)?;

    let (upper, lower_two) = root_area.split_vertically(720 / 3);
    let (middle, _bottom) = lower_two.split_vertically(720 / 3);

    draw_profits(&results, &upper)?;
    draw_da_costs(&results, &middle)?;
    draw_da_gas_prices(&results, &_bottom)?;

    Ok(())
}

fn draw_profits(
    results: &SimulationResults,
    upper: &DrawingArea<BitMapBackend, Shift>,
) -> anyhow::Result<()> {
    let profit_eth = results
        .profits
        .iter()
        .map(|profit| *profit as f64 / WEI_PER_ETH)
        .collect::<Vec<_>>();
    let cost_eth = results
        .costs
        .iter()
        .map(|cost| *cost as f64 / WEI_PER_ETH)
        .collect::<Vec<_>>();
    let reward_eth = results
        .rewards
        .iter()
        .map(|reward| *reward as f64 / WEI_PER_ETH)
        .collect::<Vec<_>>();

    let min_profit = profit_eth.iter().fold(f64::INFINITY, |a, &b| a.min(b));
    let max_profit = profit_eth.iter().fold(f64::MIN, |a, &b| a.max(b));
    let min_cost = cost_eth.iter().fold(f64::INFINITY, |a, &b| a.min(b));
    let max_cost = cost_eth.iter().fold(f64::MIN, |a, &b| a.max(b));
    let min_reward = reward_eth.iter().fold(f64::INFINITY, |a, &b| a.min(b));
    let max_reward = reward_eth.iter().fold(f64::MIN, |a, &b| a.max(b));

    let max_overall = max_profit.max(max_cost).max(max_reward);
    let min_overall = min_profit.min(min_cost).min(min_reward);

    let mut chart = ChartBuilder::on(upper)
        .caption(
            "Profit, Cost, and Reward Over Time",
            ("sans-serif", 50).into_font(),
        )
        .margin(5)
        .x_label_area_size(40)
        .y_label_area_size(60)
        .build_cartesian_2d(0..profit_eth.len(), min_overall..max_overall)?;

    chart.configure_mesh().draw()?;

    chart
        .draw_series(LineSeries::new(
            profit_eth.iter().enumerate().map(|(x, y)| (x, *y)),
            &BLACK,
        ))?
        .label("Predicted Profit ETH")
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], BLACK));

    chart
        .draw_series(LineSeries::new(
            cost_eth.iter().enumerate().map(|(x, y)| (x, *y)),
            &RED,
        ))?
        .label("Known Cost ETH")
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], RED));

    chart
        .draw_series(LineSeries::new(
            reward_eth.iter().enumerate().map(|(x, y)| (x, *y)),
            &BLUE,
        ))?
        .label("Reward ETH")
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], BLUE));

    chart
        .configure_series_labels()
        .background_style(WHITE.mix(0.8))
        .border_style(BLACK)
        .draw()?;
    Ok(())
}

fn draw_da_costs(
    results: &SimulationResults,
    lower: &DrawingArea<BitMapBackend, Shift>,
) -> anyhow::Result<()> {
    let mut chart = ChartBuilder::on(lower)
        .caption(
            "Price Per Byte to Post to DA Over Time",
            ("sans-serif", 50).into_font(),
        )
        .margin(5)
        .x_label_area_size(40)
        .y_label_area_size(60)
        .build_cartesian_2d(
            0..results.cost_per_byte.len(),
            0..*results.cost_per_byte.iter().max().unwrap() as i64,
        )?;

    chart
        .configure_mesh()
        .y_label_formatter(&|y| format!("{:e}", y))
        .draw()?;

    chart
        .draw_series(LineSeries::new(
            results
                .cost_per_byte
                .iter()
                .enumerate()
                .map(|(x, y)| (x, *y as i64)),
            &BLUE,
        ))?
        .label("Price Per Byte")
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], BLUE));

    chart
        .configure_series_labels()
        .background_style(WHITE.mix(0.8))
        .border_style(BLACK)
        .draw()?;
    Ok(())
}

fn draw_da_gas_prices(
    results: &SimulationResults,
    bottom: &DrawingArea<BitMapBackend, Shift>,
) -> anyhow::Result<()> {
    let mut chart = ChartBuilder::on(bottom)
        .caption("DA Gas Price Over Time", ("sans-serif", 50).into_font())
        .margin(5)
        .x_label_area_size(40)
        .y_label_area_size(60)
        .build_cartesian_2d(
            0..results.gas_price.len(),
            0..*results.gas_price.iter().max().unwrap() as i64,
        )?;

    chart.configure_mesh().draw()?;

    chart
        .draw_series(LineSeries::new(
            results
                .gas_price
                .iter()
                .enumerate()
                .map(|(x, y)| (x, *y as i64)),
            &BLUE,
        ))?
        .label("DA Gas Price")
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], BLUE));

    chart
        .configure_series_labels()
        .background_style(WHITE.mix(0.8))
        .border_style(BLACK)
        .draw()?;
    Ok(())
}
