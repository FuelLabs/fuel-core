use crate::txpool::PoolTransaction;
use std::{
    ops::Deref,
    sync::Arc,
};
use tai64::Tai64;

pub type ArcPoolTx = Arc<PoolTransaction>;

#[derive(Debug, Clone)]
pub struct TxInfo {
    tx: ArcPoolTx,
    submitted_time: Tai64,
}

impl TxInfo {
    pub fn new(tx: ArcPoolTx) -> Self {
        Self {
            tx,
            submitted_time: Tai64::now(),
        }
    }

    pub fn tx(&self) -> &ArcPoolTx {
        &self.tx
    }

    pub fn submitted_time(&self) -> Tai64 {
        self.submitted_time
    }
}

impl Deref for TxInfo {
    type Target = ArcPoolTx;
    fn deref(&self) -> &Self::Target {
        &self.tx
    }
}
