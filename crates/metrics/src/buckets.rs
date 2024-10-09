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
    TransactionTimeInTxPool,
    SelectTransactionTime,
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
                         1.0,
                         2.0,
                         3.0,
                         4.0,
                         5.0,
                        10.0,
                        18.0,
                        32.0,
                        57.0,
                       102.0,
                       183.0,
                       327.0,
                       853.0,
                     1_041.0,
                     1_858.0,
                     3_316.0,
                     5_918.0,
                    10_561.0,
                    18_844.0,
                    33_625.0,
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
            // Consider blocks up to 256kb in size.
            Buckets::SizeUsed,
            vec![
                  1.0 * 1024.0,
                  2.0 * 1024.0,
                  3.0 * 1024.0,
                  4.0 * 1024.0,
                  5.0 * 1024.0,
                  7.0 * 1024.0,
                 10.0 * 1024.0,
                 13.0 * 1024.0,
                 18.0 * 1024.0,
                 24.0 * 1024.0,
                 33.0 * 1024.0,
                 44.0 * 1024.0,
                 59.0 * 1024.0,
                 79.0 * 1024.0,
                106.0 * 1024.0,
                142.0 * 1024.0,
                191.0 * 1024.0,
                256.0 * 1024.0,
            ]
        ),
        (
            // Default TTL is 5m, but we want to cover a wider range.
            Buckets::TransactionTimeInTxPool,
            vec![
                       1.0,
                       2.0,
                       3.0,
                       4.0,
                       5.0,
                      10.0,
                      20.0,
                      30.0,
                      40.0,
                      50.0,
                      60.0,
                     120.0,
                     240.0,
                     480.0,
                     960.0,
                    1920.0,
                    3840.0,            
            ]
        ),
        (
            // These are nanoseconds, because we assume that the selection algorithm is fast for most of the cases.
            // If we start seeing higher values, we should analyze the performance of the selection algorithm.
            Buckets::SelectTransactionTime,
            vec![
                    10.0,
                    20.0,
                    30.0,
                    40.0,
                    50.0,
                   100.0,
                   200.0,
                   500.0,
                  1000.0,
                  2000.0,
                  5000.0,
                 10000.0,
                 50000.0,
                100000.0,
                500000.0,            ]
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
