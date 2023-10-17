pub mod alu;
pub mod blockchain;
pub mod crypto;
pub mod flow;
pub mod mem;

pub use super::run_group_ref;

use super::utils;
use core::iter::successors;

fn generate_linear_costs() -> Vec<u32> {
    let mut linear = vec![1, 10, 100, 1000, 10_000];
    let mut l = successors(Some(100_000.0f64), |n| Some(n / 1.5))
        .take(5)
        .map(|f| f as u32)
        .collect::<Vec<_>>();
    l.sort_unstable();
    linear.extend(l);
    linear
}
