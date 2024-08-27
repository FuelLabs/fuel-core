use super::*;

fn da_pid_factors(size: usize) -> Vec<(i64, i64)> {
    let mut rng = StdRng::seed_from_u64(10902);
    (0usize..size)
        .map(|_| {
            let p = rng.gen_range(0..10_000_000);
            let d = rng.gen_range(0..10_000_000);
            (p, d)
        })
        .collect()
    // vec![(85_000, 0)]
}

pub fn naive_optimisation(
    iterations: usize,
    avg_window: u32,
) -> (SimulationResults, (i64, i64, u32)) {
    da_pid_factors(iterations)
        .iter()
        .map(|(p, d)| (run_simulation(*p, *d, avg_window), (*p, *d, avg_window)))
        .min_by_key(|(results, _)| {
            let SimulationResults { actual_profit, .. } = results;
            let err = actual_profit.iter().map(|p| p.abs()).sum::<i64>();
            // println!("Error: {}", err);
            err
        })
        .unwrap()
}
