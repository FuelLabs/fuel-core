use super::*;

fn da_pid_factors(size: usize) -> Vec<(i64, i64)> {
    // let mut rng = StdRng::seed_from_u64(10902);
    // (0usize..size)
    //     .map(|_| {
    //         let p = rng.gen_range(100_000..5_000_000);
    //         let d = rng.gen_range(100_000..5_000_000);
    //         (p, d)
    //     })
    //     .collect()
    // vec![(1_700_000, 50_000)] // Better on short term
    vec![(1_700_000, 650_000)] // Safe
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
