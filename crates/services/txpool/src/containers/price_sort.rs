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
pub type PriceSort = Sort<PriceSortKey>;

#[derive(Clone, Debug)]
pub struct PriceSortKey {
    price: GasPrice,
    tx_id: TxId,
}

impl SortableKey for PriceSortKey {
    type Value = GasPrice;

    fn new(info: &TxInfo) -> Self {
        Self {
            price: info.tx().price(),
            tx_id: info.tx().id(),
        }
    }

    fn value(&self) -> &Self::Value {
        &self.price
    }

    fn tx_id(&self) -> &TxId {
        &self.tx_id
    }
}

impl PartialEq for PriceSortKey {
    fn eq(&self, other: &Self) -> bool {
        self.tx_id == other.tx_id
    }
}

impl Eq for PriceSortKey {}

impl PartialOrd for PriceSortKey {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PriceSortKey {
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        let cmp = self.price.cmp(&other.price);
        if cmp == cmp::Ordering::Equal {
            return self.tx_id.cmp(&other.tx_id)
        }
        cmp
    }
}
