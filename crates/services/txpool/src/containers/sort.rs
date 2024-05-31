use crate::TxInfo;
use fuel_core_types::services::txpool::ArcPoolTx;
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct Sort<Key> {
    /// all transactions sorted by min/max value
    pub sort: BTreeMap<Key, ArcPoolTx>,
}

impl<Key> Default for Sort<Key> {
    fn default() -> Self {
        Self {
            sort: Default::default(),
        }
    }
}

impl<Key> Sort<Key>
where
    Key: SortableKey,
{
    pub fn remove(&mut self, info: &TxInfo) {
        self.sort.remove(&Key::new(info));
    }

    pub fn lowest_tx(&self) -> Option<ArcPoolTx> {
        self.sort.iter().next().map(|(_, tx)| tx.clone())
    }

    pub fn lowest_value(&self) -> Option<Key::Value> {
        self.sort.iter().next().map(|(key, _)| key.value().clone())
    }

    pub fn lowest(&self) -> Option<(&Key, &ArcPoolTx)> {
        self.sort.iter().next()
    }

    pub fn insert(&mut self, info: &TxInfo) {
        let tx = info.tx().clone();
        self.sort.insert(Key::new(info), tx);
    }
}

pub trait SortableKey: Ord {
    type Value: Clone;

    fn new(info: &TxInfo) -> Self;

    fn value(&self) -> &Self::Value;
}
