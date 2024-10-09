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
    SizeUsed,
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
                         0.0,
                       100.0,
                       200.0,
                       400.0,
                       800.0,
                     1_600.0,
                     3_200.0,
                     6_400.0,
                    12_800.0,
                    25_600.0,
                    51_200.0,
                   102_400.0,
                   204_800.0,
                   409_600.0,
                   819_200.0,
                 1_638_400.0,
                 3_276_800.0,
                 6_553_600.0,
                13_107_200.0,
                26_214_400.0,
                52_428_800.0,
             ]
        ),
        (
            // Consider blocks up to 256kb in size, one bucket per 16kb.
            Buckets::SizeUsed,
            vec![
                 16.0 * 1024.0,
                 32.0 * 1024.0,
                 48.0 * 1024.0,
                 64.0 * 1024.0,
                 80.0 * 1024.0,
                 96.0 * 1024.0,
                112.0 * 1024.0,
                128.0 * 1024.0,
                144.0 * 1024.0,
                160.0 * 1024.0,
                176.0 * 1024.0,
                192.0 * 1024.0,
                208.0 * 1024.0,
                224.0 * 1024.0,
                240.0 * 1024.0,
                256.0 * 1024.0,
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
