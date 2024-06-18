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

// pub fn compare_the_two() {
//     let da_pid_factor = vec![(1188455853, 116635575)];
//     let one_percent = 2;
//     let one = run_simulation(da_pid_factor[0].0, da_pid_factor[0].1, 2, one_percent);
//     let two_percent = 10;
//     let two = run_simulation(da_pid_factor[0].0, da_pid_factor[0].1, 2, two_percent);
//
//     // for (i, (one_profit, two_profit)) in one
//     //     .actual_profit
//     //     .iter()
//     //     .zip(two.actual_profit.iter())
//     //     .enumerate()
//     // {
//     //     // check that the profits are the same for each and report if they are different with context
//     //     if one_profit != two_profit {
//     //         println!(
//     //             "Profit at index {} is different: {} vs {}",
//     //             i, one_profit, two_profit
//     //         );
//     //         // report all the values in the simulation results up to this index
//     //         let SimulationResults {
//     //             gas_prices,
//     //             exec_gas_prices,
//     //             da_gas_prices,
//     //             fullness,
//     //             actual_profit,
//     //             projected_profit,
//     //             pessimistic_costs,
//     //         } = one;
//     //         let SimulationResults {
//     //             gas_prices: gas_prices2,
//     //             exec_gas_prices: exec_gas_prices2,
//     //             da_gas_prices: da_gas_prices2,
//     //             fullness: fullness2,
//     //             actual_profit: actual_profit2,
//     //             projected_profit: projected_profit2,
//     //             pessimistic_costs: pessimistic_costs2,
//     //         } = two;
//     //         for j in 0..=i {
//     //             println!(
//     //                 "Index: {} Profit: {} vs {}",
//     //                 j, actual_profit[j], actual_profit2[j]
//     //             );
//     //             println!("Gas Prices: {} vs {}", gas_prices[j], gas_prices2[j]);
//     //             println!(
//     //                 "Exec Gas Prices: {} vs {}",
//     //                 exec_gas_prices[j], exec_gas_prices2[j]
//     //             );
//     //             println!(
//     //                 "DA Gas Prices: {} vs {}",
//     //                 da_gas_prices[j], da_gas_prices2[j]
//     //             );
//     //             println!("Fullness: {:?} vs {:?}", fullness[j], fullness2[j]);
//     //             println!(
//     //                 "Projected Profit: {} vs {}",
//     //                 projected_profit[j], projected_profit2[j]
//     //             );
//     //             println!(
//     //                 "Pessimistic Costs: {} vs {}",
//     //                 pessimistic_costs[j], pessimistic_costs2[j]
//     //             );
//     //         }
//     //         break;
//     //     }
//     // }
// }
