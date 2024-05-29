use crate::{
    containers::sort::{
        Sort,
        SortableKey,
    },
    types::*,
    TxInfo,
};
use core::{
    cmp,
    time::Duration,
};

/// all transactions sorted by min/max time
pub type TimeSort = Sort<TimeSortKey>;

#[derive(Clone, Debug)]
pub struct TimeSortKey {
    time: Duration,
    created: tokio::time::Instant,
    tx_id: TxId,
}

impl TimeSortKey {
    pub fn created(&self) -> &tokio::time::Instant {
        &self.created
    }
}

impl SortableKey for TimeSortKey {
    type Value = Duration;

    fn new(info: &TxInfo) -> Self {
        Self {
            time: info.submitted_time(),
            created: info.created(),
            tx_id: info.tx().id(),
        }
    }

    fn value(&self) -> &Self::Value {
        &self.time
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
        Some(self.cmp(other))
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
