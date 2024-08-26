use super::*;

pub fn draw_gas_prices(
    drawing_area: &DrawingArea<BitMapBackend, Shift>,
    gas_prices: &[u64],
    exec_gas_prices: &[u64],
    da_gas_prices: &[u64],
    title: &str,
) {
    const GAS_PRICE_COLOR: RGBColor = BLACK;
    const EXEC_GAS_PRICE_COLOR: RGBColor = RED;
    const DA_GAS_PRICE_COLOR: RGBColor = BLUE;
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
            GAS_PRICE_COLOR,
        ))
        .unwrap()
        .label("Gas Price")
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], GAS_PRICE_COLOR));

    // Draw the exec gas prices
    chart
        .draw_series(LineSeries::new(
            exec_gas_prices.iter().enumerate().map(|(x, y)| (x, *y)),
            EXEC_GAS_PRICE_COLOR,
        ))
        .unwrap()
        .label("Exec Gas Price")
        .legend(|(x, y)| {
            PathElement::new(vec![(x, y), (x + 20, y)], EXEC_GAS_PRICE_COLOR)
        });

    // Draw the da gas prices
    chart
        .draw_series(LineSeries::new(
            da_gas_prices.iter().enumerate().map(|(x, y)| (x, *y)),
            DA_GAS_PRICE_COLOR,
        ))
        .unwrap()
        .label("DA Gas Price")
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], DA_GAS_PRICE_COLOR));

    chart
        .configure_series_labels()
        .background_style(WHITE.mix(0.8))
        .border_style(BLACK)
        .draw()
        .unwrap();
}

pub fn draw_fullness(
    drawing_area: &DrawingArea<BitMapBackend, Shift>,
    fullness: &Vec<(u64, u64)>,
    title: &str,
) {
    const FULLNESS_COLOR: RGBColor = BLACK;

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
            FULLNESS_COLOR,
        ))
        .unwrap()
        .label("Fullness")
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], FULLNESS_COLOR));

    chart
        .configure_series_labels()
        .background_style(WHITE.mix(0.8))
        .border_style(BLACK)
        .draw()
        .unwrap();
}

pub fn draw_bytes_and_cost_per_block(
    drawing_area: &DrawingArea<BitMapBackend, Shift>,
    bytes_and_costs_per_block: &[(u64, u64)],
    title: &str,
) {
    const BYTES_PER_BLOCK_COLOR: RGBColor = BLACK;
    let (bytes, costs): (Vec<u64>, Vec<u64>) = bytes_and_costs_per_block.iter().cloned().unzip();

    let min = 0;
    let max_left = *bytes.iter().max().unwrap();
    let max_right = *costs.iter().max().unwrap();

    let mut chart = ChartBuilder::on(drawing_area)
        .caption(title, ("sans-serif", 50).into_font())
        .margin(5)
        .x_label_area_size(40)
        .y_label_area_size(60)
        .right_y_label_area_size(40)
        .build_cartesian_2d(0..bytes_and_costs_per_block.len(), min..max_left)
        .unwrap()
        .set_secondary_coord(0..bytes_and_costs_per_block.len(), min..max_right);

    chart
        .configure_mesh()
        .y_desc("Bytes Per Block")
        .x_desc("Block")
        .draw()
        .unwrap();

    chart.configure_secondary_axes().y_desc("Cost Per Block").draw().unwrap();

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

    chart.draw_secondary_series(LineSeries::new(
        costs.iter().enumerate().map(|(x, y)| (x, *y)),
        RED,
    )).unwrap().label("Cost Per Block").legend(|(x, y)| {
        PathElement::new(vec![(x, y), (x + 20, y)], RED)
    });

    chart
        .configure_series_labels()
        .background_style(WHITE.mix(0.8))
        .border_style(BLACK)
        .draw()
        .unwrap();
}

pub fn draw_profit(
    drawing_area: &DrawingArea<BitMapBackend, Shift>,
    actual_profit: &[i64],
    projected_profit: &[i64],
    pessimistic_block_costs: &[u64],
    title: &str,
) {
    const ACTUAL_PROFIT_COLOR: RGBColor = BLACK;
    const PROJECTED_PROFIT_COLOR: RGBColor = RED;
    const PESSIMISTIC_BLOCK_COST_COLOR: RGBColor = BLUE;
    let min = *actual_profit.iter().min().unwrap();
    let max = *actual_profit.iter().max().unwrap();
    println!("min: {}, max: {}", min, max);

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
            ACTUAL_PROFIT_COLOR,
        ))
        .unwrap()
        .label("Actual Profit")
        .legend(|(x, y)| {
            PathElement::new(vec![(x, y), (x + 20, y)], ACTUAL_PROFIT_COLOR)
        });

    chart
        .draw_series(LineSeries::new(
            projected_profit.iter().enumerate().map(|(x, y)| (x, *y)),
            PROJECTED_PROFIT_COLOR,
        ))
        .unwrap()
        .label("Projected Profit")
        .legend(|(x, y)| {
            PathElement::new(vec![(x, y), (x + 20, y)], PROJECTED_PROFIT_COLOR)
        });

    // draw the block bytes
    chart
        .draw_series(LineSeries::new(
            pessimistic_block_costs
                .iter()
                .enumerate()
                .map(|(x, y)| (x, *y as i64)),
            PESSIMISTIC_BLOCK_COST_COLOR,
        ))
        .unwrap()
        .label("Pessimistic Block Costs")
        .legend(|(x, y)| {
            PathElement::new(vec![(x, y), (x + 20, y)], PESSIMISTIC_BLOCK_COST_COLOR)
        });

    chart
        .configure_series_labels()
        .background_style(WHITE.mix(0.8))
        .border_style(BLACK)
        .draw()
        .unwrap();
}
