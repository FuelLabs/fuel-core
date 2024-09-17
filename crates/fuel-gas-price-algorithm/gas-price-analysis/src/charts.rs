use super::*;
use std::{
    fs,
    path::PathBuf,
};

pub fn draw_chart(
    results: SimulationResults,
    p_comp: i64,
    d_comp: i64,
    file_path: &str,
) -> anyhow::Result<()> {
    let SimulationResults {
        gas_prices,
        exec_gas_prices,
        da_gas_prices,
        fullness,
        bytes_and_costs,
        actual_profit,
        projected_profit,
        pessimistic_costs,
    } = results;

    let plot_width = 640 * 2 * 2;
    let plot_height = 480 * 3;

    let path: PathBuf = file_path.into();

    if path.is_dir() {
        println!("Creating chart at: {}", file_path);
        fs::create_dir_all(file_path)?;
    } else {
        let new_path = path.parent().ok_or(anyhow::anyhow!("Path has no parent"))?;
        println!("Creating chart at: {}", new_path.display());
        fs::create_dir_all(new_path)?;
    }
    let root =
        BitMapBackend::new(file_path, (plot_width, plot_height)).into_drawing_area();
    root.fill(&WHITE).unwrap();
    let (window_one, lower) = root.split_vertically(plot_height / 4);
    let (window_two, new_lower) = lower.split_vertically(plot_height / 4);
    let (window_three, window_four) = new_lower.split_vertically(plot_height / 4);

    draw_fullness(&window_one, &fullness, "Fullness")?;

    draw_bytes_and_cost_per_block(&window_two, &bytes_and_costs, "Bytes Per Block")?;

    draw_profit(
        &window_three,
        &actual_profit,
        &projected_profit,
        &pessimistic_costs,
        &format!(
            "Profit p_comp: {}, d_comp: {}",
            prettify_number(p_comp),
            prettify_number(d_comp)
        ),
    )?;
    draw_gas_prices(
        &window_four,
        &gas_prices,
        &exec_gas_prices,
        &da_gas_prices,
        "Gas Prices",
    )?;

    root.present()?;

    Ok(())
}

pub fn draw_gas_prices(
    drawing_area: &DrawingArea<BitMapBackend, Shift>,
    gas_prices: &[u64],
    _exec_gas_prices: &[u64],
    da_gas_prices: &[u64],
    title: &str,
) -> anyhow::Result<()> {
    // const GAS_PRICE_COLOR: RGBColor = BLACK;
    // const EXEC_GAS_PRICE_COLOR: RGBColor = RED;
    const DA_GAS_PRICE_COLOR: RGBColor = BLUE;
    let min = 0;
    let max = *da_gas_prices.iter().max().unwrap();

    let mut chart = ChartBuilder::on(drawing_area)
        .caption(title, ("sans-serif", 50).into_font())
        .margin(5)
        .x_label_area_size(40)
        .y_label_area_size(100)
        .right_y_label_area_size(100)
        .build_cartesian_2d(0..gas_prices.len(), min..max)?;

    chart
        .configure_mesh()
        .y_desc("DA Gas Price")
        .x_desc("Block")
        .draw()?;

    // chart
    //     .draw_series(LineSeries::new(
    //         gas_prices.iter().enumerate().map(|(x, y)| (x, *y)),
    //         GAS_PRICE_COLOR,
    //     ))
    //     .unwrap()
    //     .label("Gas Price")
    //     .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], GAS_PRICE_COLOR));

    // Draw the exec gas prices
    // chart
    //     .draw_series(LineSeries::new(
    //         exec_gas_prices.iter().enumerate().map(|(x, y)| (x, *y)),
    //         EXEC_GAS_PRICE_COLOR,
    //     ))
    //     .unwrap()
    //     .label("Exec Gas Price")
    //     .legend(|(x, y)| {
    //         PathElement::new(vec![(x, y), (x + 20, y)], EXEC_GAS_PRICE_COLOR)
    //     });

    // Draw the da gas prices
    chart
        .draw_series(LineSeries::new(
            da_gas_prices.iter().enumerate().map(|(x, y)| (x, *y)),
            DA_GAS_PRICE_COLOR,
        ))?
        .label("DA Gas Price")
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], DA_GAS_PRICE_COLOR));

    chart
        .configure_series_labels()
        .background_style(WHITE.mix(0.8))
        .border_style(BLACK)
        .draw()?;

    Ok(())
}

pub fn draw_fullness(
    drawing_area: &DrawingArea<BitMapBackend, Shift>,
    fullness: &Vec<(u64, u64)>,
    title: &str,
) -> anyhow::Result<()> {
    const FULLNESS_COLOR: RGBColor = BLACK;

    let min = 0;
    let max = 100;

    let mut chart = ChartBuilder::on(drawing_area)
        .caption(title, ("sans-serif", 50).into_font())
        .margin(5)
        .x_label_area_size(40)
        .y_label_area_size(100)
        .right_y_label_area_size(100)
        .build_cartesian_2d(0..fullness.len(), min..max)?;

    chart
        .configure_mesh()
        .y_desc("Fullness Percentage")
        .x_desc("Block")
        .draw()?;

    chart
        .draw_series(LineSeries::new(
            fullness
                .iter()
                .map(|(x, y)| (*x as f64 / *y as f64) * 100.)
                .map(|x| x as i32)
                .enumerate(),
            FULLNESS_COLOR,
        ))
        .unwrap()
        .label("Fullness")
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], FULLNESS_COLOR));

    chart
        .configure_series_labels()
        .background_style(WHITE.mix(0.8))
        .border_style(BLACK)
        .draw()?;

    Ok(())
}

pub fn draw_bytes_and_cost_per_block(
    drawing_area: &DrawingArea<BitMapBackend, Shift>,
    bytes_and_costs_per_block: &[(u64, u64)],
    title: &str,
) -> anyhow::Result<()> {
    const BYTES_PER_BLOCK_COLOR: RGBColor = BLACK;
    let (bytes, costs): (Vec<u64>, Vec<u64>) =
        bytes_and_costs_per_block.iter().cloned().unzip();

    let min = 0;
    let max_left = *bytes.iter().max().unwrap();
    let max_right = *costs.iter().max().unwrap();

    let mut chart = ChartBuilder::on(drawing_area)
        .caption(title, ("sans-serif", 50).into_font())
        .margin(5)
        .x_label_area_size(40)
        .y_label_area_size(100)
        .right_y_label_area_size(100)
        .build_cartesian_2d(0..bytes_and_costs_per_block.len(), min..max_left)
        .unwrap()
        .set_secondary_coord(0..bytes_and_costs_per_block.len(), min..max_right);

    chart
        .configure_mesh()
        .y_desc("Bytes Per Block")
        .x_desc("Block")
        .draw()
        .unwrap();

    chart
        .configure_secondary_axes()
        .y_desc("Cost Per Block")
        .draw()
        .unwrap();

    chart
        .draw_series(LineSeries::new(
            bytes.iter().enumerate().map(|(x, y)| (x, *y)),
            BYTES_PER_BLOCK_COLOR,
        ))
        .unwrap()
        .label("Bytes Per Block")
        .legend(|(x, y)| {
            PathElement::new(vec![(x, y), (x + 20, y)], BYTES_PER_BLOCK_COLOR)
        });

    chart
        .draw_secondary_series(LineSeries::new(
            costs.iter().enumerate().map(|(x, y)| (x, *y)),
            RED,
        ))
        .unwrap()
        .label("Cost Per Block")
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], RED));

    chart
        .configure_series_labels()
        .background_style(WHITE.mix(0.8))
        .border_style(BLACK)
        .draw()
        .unwrap();

    Ok(())
}

pub fn draw_profit(
    drawing_area: &DrawingArea<BitMapBackend, Shift>,
    actual_profit: &[i128],
    projected_profit: &[i128],
    pessimistic_block_costs: &[u128],
    title: &str,
) -> anyhow::Result<()> {
    const ACTUAL_PROFIT_COLOR: RGBColor = BLACK;
    const PROJECTED_PROFIT_COLOR: RGBColor = RED;
    const PESSIMISTIC_BLOCK_COST_COLOR: RGBColor = BLUE;
    let min = *std::cmp::min(
        actual_profit
            .iter()
            .min()
            .ok_or(anyhow::anyhow!("Path has no parent"))?,
        projected_profit
            .iter()
            .min()
            .ok_or(anyhow::anyhow!("Path has no parent"))?,
    );
    let max = *std::cmp::max(
        actual_profit
            .iter()
            .max()
            .ok_or(anyhow::anyhow!("Path has no parent"))?,
        projected_profit
            .iter()
            .max()
            .ok_or(anyhow::anyhow!("Path has no parent"))?,
    );

    let mut chart = ChartBuilder::on(drawing_area)
        .caption(title, ("sans-serif", 50).into_font())
        .margin(5)
        .x_label_area_size(40)
        .y_label_area_size(100)
        .right_y_label_area_size(100)
        .build_cartesian_2d(0..actual_profit.len(), min..max)
        .unwrap()
        .set_secondary_coord(
            0..actual_profit.len(),
            0..*pessimistic_block_costs.iter().max().unwrap(),
        );

    chart
        .configure_mesh()
        .y_desc("Profit")
        .x_desc("Block")
        .draw()?;

    chart
        .configure_secondary_axes()
        .y_desc("Pessimistic cost")
        .draw()?;

    chart
        .draw_series(LineSeries::new(
            actual_profit.iter().enumerate().map(|(x, y)| (x, *y)),
            ACTUAL_PROFIT_COLOR,
        ))?
        .label("Actual Profit")
        .legend(|(x, y)| {
            PathElement::new(vec![(x, y), (x + 20, y)], ACTUAL_PROFIT_COLOR)
        });

    chart
        .draw_series(LineSeries::new(
            projected_profit.iter().enumerate().map(|(x, y)| (x, *y)),
            PROJECTED_PROFIT_COLOR,
        ))?
        .label("Projected Profit")
        .legend(|(x, y)| {
            PathElement::new(vec![(x, y), (x + 20, y)], PROJECTED_PROFIT_COLOR)
        });

    // draw the block bytes
    chart
        .draw_secondary_series(LineSeries::new(
            pessimistic_block_costs
                .iter()
                .enumerate()
                .map(|(x, y)| (x, *y)),
            PESSIMISTIC_BLOCK_COST_COLOR,
        ))?
        .label("Pessimistic Block Costs")
        .legend(|(x, y)| {
            PathElement::new(vec![(x, y), (x + 20, y)], PESSIMISTIC_BLOCK_COST_COLOR)
        });

    chart
        .configure_series_labels()
        .background_style(WHITE.mix(0.8))
        .border_style(BLACK)
        .draw()?;

    Ok(())
}
