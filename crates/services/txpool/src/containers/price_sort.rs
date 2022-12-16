use crate::types::*;
use fuel_core_types::services::txpool::ArcPoolTx;
use std::{
    cmp,
    collections::BTreeMap,
};

#[derive(Debug, Default, Clone)]
pub struct PriceSort {
    /// all transactions sorted by min/max value
    pub sort: BTreeMap<PriceSortKey, ArcPoolTx>,
}

impl PriceSort {
    pub fn remove(&mut self, tx: &ArcPoolTx) {
        self.sort.remove(&PriceSortKey::new(tx));
    }

    // get last transaction. It has lowest gas price.
    pub fn last(&self) -> Option<ArcPoolTx> {
        self.sort.iter().next().map(|(_, tx)| tx.clone())
    }

    pub fn lowest_price(&self) -> GasPrice {
        self.sort
            .iter()
            .next()
            .map(|(price, _)| price.price)
            .unwrap_or_default()
    }

    pub fn insert(&mut self, tx: &ArcPoolTx) {
        self.sort.insert(PriceSortKey::new(tx), tx.clone());
    }
}

#[derive(Clone, Debug)]
pub struct PriceSortKey {
    price: GasPrice,
    tx_id: TxId,
}

impl PriceSortKey {
    pub fn new(tx: &ArcPoolTx) -> Self {
        Self {
            price: tx.price(),
            tx_id: tx.id(),
        }
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
        match self.price.partial_cmp(&other.price) {
            Some(core::cmp::Ordering::Equal) => {}
            ord => return ord,
        }
        self.tx_id.partial_cmp(&other.tx_id)
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
