use std::{
    collections::HashMap,
    sync::OnceLock,
};
#[cfg(test)]
use strum_macros::EnumIter;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(test, derive(EnumIter))]
pub(crate) enum Buckets {
    Timing,
    GasUsed,
    TransactionsCount,
    Fee,
}
static BUCKETS: OnceLock<HashMap<Buckets, Vec<f64>>> = OnceLock::new();
pub(crate) fn buckets(b: Buckets) -> impl Iterator<Item = f64> {
    BUCKETS.get_or_init(initialize_buckets)[&b].iter().copied()
}

#[rustfmt::skip]
fn initialize_buckets() -> HashMap<Buckets, Vec<f64>> {
    [
        (
            Buckets::Timing,
            vec![
                0.005,
                0.010,
                0.025,
                0.050,
                0.100,
                0.250,
                0.500,
                1.000,
                2.500,
                5.000,
               10.000,
            ],
        ),
        (
            // Gas limit is 30_000_000.
            Buckets::GasUsed,
            vec![
                    10_000.0,
                    25_000.0,
                    50_000.0,
                   100_000.0,
                   250_000.0,
                   500_000.0,
                 1_000_000.0,
                 3_000_000.0,
                 7_500_000.0,
                15_000_000.0,
                25_000_000.0,
            ],
        ),
        (
            // Transaction count is fixed at 65_535.
            Buckets::TransactionsCount,
            vec![
                     5.0,
                    10.0,
                    25.0,
                    50.0,
                   100.0,
                 1_000.0,
                 5_000.0,
                10_000.0,
                20_000.0,
                40_000.0,
                60_000.0,
            ],
        ),
        (
            // 50 ETH is expected to be the realistic maximum fee for the time being.
            Buckets::Fee,
            vec![
                    50_000_000.0,
                   400_000_000.0,
                 1_350_000_000.0,
                 3_200_000_000.0,
                 6_250_000_000.0,
                10_800_000_000.0,
                17_150_000_000.0,
                25_600_000_000.0,
                36_450_000_000.0,
                50_000_000_000.0,
            ]
        ),
    ]
    .into_iter()
    .collect()
}

#[cfg(test)]
mod tests {
    use strum::IntoEnumIterator;

    use crate::buckets::Buckets;

    use super::initialize_buckets;

    #[test]
    fn buckets_are_defined_for_every_variant() {
        let actual_buckets = initialize_buckets();
        let actual_buckets = actual_buckets.keys().collect::<Vec<_>>();

        let required_buckets: Vec<_> = Buckets::iter().collect();

        assert_eq!(required_buckets.len(), actual_buckets.len());

        let all_buckets_defined = required_buckets
            .iter()
            .all(|required_bucket| actual_buckets.contains(&required_bucket));

        assert!(all_buckets_defined)
    }
}
