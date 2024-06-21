use num_rational::Ratio;
use tokio::time::Instant;

use crate::{
    containers::sort::{
        Sort,
        SortableKey,
    },
    types::*,
    TxInfo,
};
use std::cmp;

/// All transactions sorted by `RatioMaxGasPriceSortKey`.
pub type RatioMaxGasPriceSort = Sort<RatioMaxGasPriceSortKey>;

/// A ratio between tip adn max fee.
type RatioKey = Ratio<Word>;

#[derive(Clone, Debug)]
pub struct RatioMaxGasPriceSortKey {
    fee_per_gas: RatioKey,
    creation_instant: Instant,
    tx_id: TxId,
}

impl SortableKey for RatioMaxGasPriceSortKey {
    type Value = RatioKey;

    fn new(info: &TxInfo) -> Self {
        Self {
            fee_per_gas: Ratio::new(
                info.tx().max_fee().saturating_sub(info.tx.tip()),
                info.tx().max_gas(),
            ),
            creation_instant: info.created(),
            tx_id: info.tx().id(),
        }
    }

    fn value(&self) -> &Self::Value {
        &self.fee_per_gas
    }
}

impl PartialEq for RatioMaxGasPriceSortKey {
    fn eq(&self, other: &Self) -> bool {
        self.tx_id == other.tx_id
    }
}

impl Eq for RatioMaxGasPriceSortKey {}

impl PartialOrd for RatioMaxGasPriceSortKey {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for RatioMaxGasPriceSortKey {
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        let cmp = self.fee_per_gas.cmp(&other.fee_per_gas);
        if cmp == cmp::Ordering::Equal {
            let instant_cmp = other.creation_instant.cmp(&self.creation_instant);
            if instant_cmp == cmp::Ordering::Equal {
                self.tx_id.cmp(&other.tx_id)
            } else {
                instant_cmp
            }
        } else {
            cmp
        }
    }
}
