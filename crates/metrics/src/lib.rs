#![deny(clippy::arithmetic_side_effects)]
#![deny(clippy::cast_possible_truncation)]
#![deny(unused_crate_dependencies)]
#![deny(warnings)]

use std::sync::OnceLock;

pub mod core_metrics;
pub mod future_tracker;
pub mod graphql_metrics;
pub mod importer;
pub mod p2p_metrics;
pub mod response;
pub mod services;
pub mod txpool_metrics;

// recommended bucket defaults for logging response times
static BUCKETS: OnceLock<Vec<f64>> = OnceLock::new();
pub fn timing_buckets() -> &'static Vec<f64> {
    BUCKETS.get_or_init(|| {
        vec![
            0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
        ]
    })
}
