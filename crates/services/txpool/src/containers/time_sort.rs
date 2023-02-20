use crate::{
    containers::sort::{
        Sort,
        SortableKey,
    },
    types::*,
};
use fuel_core_types::{
    services::txpool::TxInfo,
    tai64::Tai64,
};
use std::cmp;

/// all transactions sorted by min/max time
pub type TimeSort = Sort<TimeSortKey>;

#[derive(Clone, Debug)]
pub struct TimeSortKey {
    time: Tai64,
    tx_id: TxId,
}

impl SortableKey for TimeSortKey {
    type Value = Tai64;

    fn new(info: &TxInfo) -> Self {
        Self {
            time: info.submitted_time(),
            tx_id: info.tx().id(),
        }
    }

    fn value(&self) -> &Self::Value {
        &self.time
    }

    fn tx_id(&self) -> &TxId {
        &self.tx_id
    }
}

impl PartialEq for TimeSortKey {
    fn eq(&self, other: &Self) -> bool {
        self.tx_id == other.tx_id
    }
}

impl Eq for TimeSortKey {}

impl PartialOrd for TimeSortKey {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        match self.time.partial_cmp(&other.time) {
            Some(core::cmp::Ordering::Equal) => {}
            ord => return ord,
        }
        self.tx_id.partial_cmp(&other.tx_id)
    }
}

impl Ord for TimeSortKey {
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        let cmp = self.time.cmp(&other.time);
        if cmp == cmp::Ordering::Equal {
            return self.tx_id.cmp(&other.tx_id)
        }
        cmp
    }
}
