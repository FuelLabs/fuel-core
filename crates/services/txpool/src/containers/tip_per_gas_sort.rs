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

/// all transactions sorted by min/max price
pub type RatioGasTipSort = Sort<RatioGasTipSortKey>;

/// A ratio between gas and tip
pub type RatioGasTip = Ratio<Word>;

#[derive(Clone, Debug)]
pub struct RatioGasTipSortKey {
    tip_per_gas: RatioGasTip,
    creation_instant: Instant,
    tx_id: TxId,
}

impl SortableKey for RatioGasTipSortKey {
    type Value = RatioGasTip;

    fn new(info: &TxInfo) -> Self {
        Self {
            tip_per_gas: Ratio::new(info.tx().tip(), info.tx().max_gas()),
            creation_instant: info.created(),
            tx_id: info.tx().id(),
        }
    }

    fn value(&self) -> &Self::Value {
        &self.tip_per_gas
    }
}

impl PartialEq for RatioGasTipSortKey {
    fn eq(&self, other: &Self) -> bool {
        self.tx_id == other.tx_id
    }
}

impl Eq for RatioGasTipSortKey {}

impl PartialOrd for RatioGasTipSortKey {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for RatioGasTipSortKey {
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        let cmp = self.tip_per_gas.cmp(&other.tip_per_gas);
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
