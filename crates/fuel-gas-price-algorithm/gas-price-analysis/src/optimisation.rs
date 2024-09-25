use super::*;
use futures::future::join_all;

fn da_pid_factors(size: usize) -> Vec<(i64, i64)> {
    let mut rng = StdRng::seed_from_u64(10902);
    (0usize..size)
        .map(|_| {
            let p = rng.gen_range(100_000..5_000_000);
            let d = rng.gen_range(100_000..5_000_000);
            (p, d)
        })
        .collect()
}

pub async fn naive_optimisation(
    simulator: &Simulator,
    iterations: usize,
    da_recording_rate: usize,
) -> (SimulationResults, (i64, i64)) {
    let tasks = da_pid_factors(iterations)
        .into_iter()
        .map(|(p, d)| {
            let new_simulator = simulator.clone();
            let f = move || {
                (
                    new_simulator.run_simulation(p, d, da_recording_rate),
                    (p, d),
                )
            };
            tokio::task::spawn_blocking(f)
        })
        .collect::<Vec<_>>();
    join_all(tasks)
        .await
        .into_iter()
        .map(Result::unwrap)
        .min_by_key(|(results, _)| {
            let SimulationResults { actual_profit, .. } = results;
            let err = actual_profit.iter().map(|p| p.abs()).sum::<i128>();
            err
        })
        .unwrap()
}
