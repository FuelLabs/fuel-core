use super::*;

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

pub fn naive_optimisation(iterations: usize, size: usize) -> (SimulationResults, (i64, i64)) {
    da_pid_factors(iterations)
        .iter()
        .map(|(p, d)| (run_simulation(*p, *d, size), (*p, *d)))
        .min_by_key(|(results, _)| {
            let SimulationResults { actual_profit, .. } = results;
            let err = actual_profit.iter().map(|p| p.abs()).sum::<i64>();
            err
        })
        .unwrap()
}
