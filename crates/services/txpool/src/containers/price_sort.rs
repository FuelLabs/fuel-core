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
pub type TipSort = Sort<TipSortKey>;

#[derive(Clone, Debug)]
pub struct TipSortKey {
    tip: Word,
    tx_id: TxId,
}

impl SortableKey for TipSortKey {
    type Value = GasPrice;

    fn new(info: &TxInfo) -> Self {
        Self {
            tip: info.tx().tip(),
            tx_id: info.tx().id(),
        }
    }

    fn value(&self) -> &Self::Value {
        &self.tip
    }

    fn tx_id(&self) -> &TxId {
        &self.tx_id
    }
}

impl PartialEq for TipSortKey {
    fn eq(&self, other: &Self) -> bool {
        self.tx_id == other.tx_id
    }
}

impl Eq for TipSortKey {}

impl PartialOrd for TipSortKey {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for TipSortKey {
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        let cmp = self.tip.cmp(&other.tip);
        if cmp == cmp::Ordering::Equal {
            return self.tx_id.cmp(&other.tx_id)
        }
        cmp
    }
}
