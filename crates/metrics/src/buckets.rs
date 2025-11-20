use std::{collections::HashMap, sync::OnceLock};
#[cfg(test)]
use strum_macros::EnumIter;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(test, derive(EnumIter))]
pub(crate) enum Buckets {
    Timing,
    TransactionSize,
    TransactionInsertionTimeInThreadPool,
    SelectTransactionsTime,
    TransactionTimeInTxpool,
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
            // We consider blocks up to 256kb in size and single transaction can take any of this space.
            Buckets::TransactionSize,
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
            Buckets::TransactionInsertionTimeInThreadPool,
            vec![
                       50.0,
                      250.0,
                     1000.0,
                    10000.0,
                   100000.0,
                   300000.0,
                1_000_000.0,
                5_000_000.0,
            ]
        ),
        (
            Buckets::SelectTransactionsTime,
            vec![
                       50.0,
                      250.0,
                     1000.0,
                    10000.0,
                   100000.0,
                   300000.0,
                1_000_000.0,
                5_000_000.0,
            ]
        ),
        (
            Buckets::TransactionTimeInTxpool,
            vec![
                      1.0,
                      2.0,
                      5.0,
                     10.0,
                    100.0,
                    250.0,
                    600.0
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
