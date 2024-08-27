use plotters::prelude::*;
use rand::{
    rngs::StdRng,
    Rng,
    SeedableRng,
};

use plotters::coord::Shift;

use crate::{
    charts::{
        draw_bytes_and_cost_per_block,
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
    let optimisation_iterations = 10_000;
    let avg_window = 2;
    let (best, (p_comp, d_comp, avg_window)) =
        naive_optimisation(optimisation_iterations, avg_window);
    let SimulationResults {
        gas_prices,
        exec_gas_prices,
        da_gas_prices,
        fullness,
        bytes_and_costs,
        actual_profit,
        projected_profit,
        pessimistic_costs,
    } = best;

    let plot_width = 640 * 2 * 2;
    let plot_height = 480 * 3;

    const FILE_PATH: &str = "gas_prices.png";

    let root =
        BitMapBackend::new(FILE_PATH, (plot_width, plot_height)).into_drawing_area();
    root.fill(&WHITE).unwrap();
    let (window_one, lower) = root.split_vertically(plot_height / 4);
    let (window_two, new_lower) = lower.split_vertically(plot_height / 4);
    let (window_three, window_four) = new_lower.split_vertically(plot_height / 4);

    draw_fullness(&window_one, &fullness, "Fullness");

    draw_bytes_and_cost_per_block(&window_two, &bytes_and_costs, "Bytes Per Block");

    draw_profit(
        &window_three,
        &actual_profit,
        &projected_profit,
        &pessimistic_costs,
        &format!(
            "Profit p_comp: {p_comp:?}, d_comp: {d_comp:?}, avg window: {avg_window:?}"
        ),
    );
    draw_gas_prices(
        &window_four,
        &gas_prices,
        &exec_gas_prices,
        &da_gas_prices,
        "Gas Prices",
    );

    root.present().unwrap();
}
