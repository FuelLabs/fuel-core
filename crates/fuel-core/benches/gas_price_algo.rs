use criterion::{
    criterion_group,
    criterion_main,
    Criterion,
};
use fuel_core::service::adapters::fuel_gas_price_provider::{
    algorithm_adapter::FuelGasPriceAlgorithm,
    ports::{
        BlockFullness,
        GasPriceAlgorithm,
    },
};

use plotters::prelude::*;
use std::path::Path;

fn gas_price_algo(_c: &mut Criterion) {
    // vary the gas price between 10_000 and 30_000
    let da_recording_cost = (0u64..1000)
        .map(|val| f64::try_from(val).unwrap() / 100.)
        .map(f64::sin)
        .map(|x| x * 10_000. + 20_000.)
        .map(|x| x as u64);
    // for price in da_gas_prices {
    //     dbg!(price);
    // }
    let algo = FuelGasPriceAlgorithm::new(1., f32::MAX);
    let mut gas_price = 100;
    let mut gas_prices = vec![gas_price];
    let gas_spent = 200;
    let block_fullness = BlockFullness::new(50, 100);
    for cost in da_recording_cost {
        let gas_reward = gas_price * gas_spent;
        gas_price = algo.calculate_gas_price(gas_price, gas_reward, cost, block_fullness);
        gas_prices.push(gas_price);
    }

    for cost in da_recording_cost {
        let gas_reward = gas_price * gas_spent;
        gas_price = algo.calculate_gas_price(gas_price, gas_reward, cost, block_fullness);
        gas_prices.push(gas_price);
    }

    // Plotting code starts here
    let root = BitMapBackend::new("gas_prices.png", (640, 480)).into_drawing_area();
    root.fill(&WHITE).unwrap();

    let mut chart = ChartBuilder::on(&root)
        .caption("Gas Prices Over Time", ("sans-serif", 50).into_font())
        .margin(5)
        .x_label_area_size(30)
        .y_label_area_size(30)
        .build_ranged(0..gas_prices.len(), 0..*gas_prices.iter().max().unwrap())
        .unwrap();

    chart.configure_mesh().draw().unwrap();

    chart
        .draw_series(LineSeries::new(
            gas_prices.iter().enumerate().map(|(x, y)| (x, *y)),
            &RED,
        ))
        .unwrap()
        .label("Gas Price")
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], &RED));

    chart
        .configure_series_labels()
        .background_style(&WHITE.mix(0.8))
        .border_style(&BLACK)
        .draw()
        .unwrap();
}

criterion_group!(benches, gas_price_algo);
criterion_main!(benches);
