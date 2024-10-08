use std::{
    collections::HashMap,
    sync::OnceLock,
};

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub(crate) enum Buckets {
    Timing,
    GasUsed,
    TransactionsCount,
    Fee,
    GasPrice,
}
static BUCKETS: OnceLock<HashMap<Buckets, Vec<f64>>> = OnceLock::new();
pub(crate) fn buckets(b: Buckets) -> impl Iterator<Item = f64> {
    BUCKETS.get_or_init(initialize_buckets)[&b].iter().copied()
}

fn initialize_buckets() -> HashMap<Buckets, Vec<f64>> {
    [
        (
            Buckets::Timing,
            vec![
                0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
            ],
        ),
        (
            Buckets::GasUsed,
            vec![
                10000.0, 25000.0, 50000.0, 100000.0, 500000.0, 1000000.0, 1875000.0,
                3750000.0, 7500000.0, 15000000.0, 30000000.0,
            ],
        ),
        (
            Buckets::TransactionsCount,
            vec![
                5.0, 10.0, 25.0, 50.0, 100.0, 1000.0, 5000.0, 10000.0, 15000.0, 30000.0,
                65535.0,
            ],
        ),
        (
            // TODO[RC]: Uses the same values as gas_used_buckets for now. We should probably change this.
            Buckets::Fee,
            vec![
                10000.0, 25000.0, 50000.0, 100000.0, 500000.0, 1000000.0, 1875000.0,
                3750000.0, 7500000.0, 15000000.0, 30000000.0,
            ],
        ),
        (
            // TODO[RC]: Uses the same values as gas_used_buckets for now. We should probably change this.
            Buckets::GasPrice,
            vec![
                10000.0, 25000.0, 50000.0, 100000.0, 500000.0, 1000000.0, 1875000.0,
                3750000.0, 7500000.0, 15000000.0, 30000000.0,
            ],
        ),
    ]
    .into_iter()
    .collect()
}
