use plotters::prelude::*;
use rand::{
    rngs::StdRng,
    Rng,
    SeedableRng,
};

use plotters::coord::Shift;

use crate::{
    charts::{
        draw_fullness,
        draw_gas_prices,
        draw_profit,
    },
    optimisation::naive_optimisation,
    simulation::{
        run_simulation,
        SimulationResults,
    },
};

mod optimisation;
mod simulation;

mod charts;

fn main() {
    let optimisation_iterations = 50_000;
    let avg_window = 2;
    let (best, (p_comp, d_comp, avg_window)) =
        naive_optimisation(optimisation_iterations, avg_window);
    let SimulationResults {
        gas_prices,
        exec_gas_prices,
        da_gas_prices,
        fullness,
        actual_profit,
        projected_profit,
        pessimistic_costs,
    } = best;

    let plot_width = 640 * 2;
    let plot_height = 480 * 3;

    const FILE_PATH: &str = "gas_prices.png";

    let root =
        BitMapBackend::new(FILE_PATH, (plot_width, plot_height)).into_drawing_area();
    root.fill(&WHITE).unwrap();
    let (upper, lower) = root.split_vertically(plot_height / 3);
    let (middle, bottom) = lower.split_vertically(plot_height / 3);

    draw_fullness(&upper, &fullness, "Fullness");

    draw_profit(
        &middle,
        &actual_profit,
        &projected_profit,
        &pessimistic_costs,
        &format!(
            "Profit p_comp: {p_comp:?}, d_comp: {d_comp:?}, avg window: {avg_window:?}"
        ),
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
