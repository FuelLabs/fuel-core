use super::*;

fn da_pid_factors(size: usize) -> Vec<(i64, i64)> {
    let mut rng = StdRng::seed_from_u64(10902);
    (0usize..size)
        .map(|_| {
            let p = rng.gen_range(1_000..100_000_000_000);
            let d = rng.gen_range(1_000..100_000_000_000);
            (p, d)
        })
        .collect()
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
            actual_profit.iter().map(|p| p.abs()).sum::<i64>()
        })
        .unwrap()
}
