use crate::txpool::PoolTransaction;
use chrono::{
    DateTime,
    Utc,
};
use std::{
    ops::Deref,
    sync::Arc,
};

pub type ArcPoolTx = Arc<PoolTransaction>;

#[derive(Debug, Clone)]
pub struct TxInfo {
    tx: ArcPoolTx,
    submitted_time: DateTime<Utc>,
}

impl TxInfo {
    pub fn new(tx: ArcPoolTx) -> Self {
        Self {
            tx,
            submitted_time: Utc::now(),
        }
    }

    pub fn tx(&self) -> &ArcPoolTx {
        &self.tx
    }

    pub fn submitted_time(&self) -> DateTime<Utc> {
        self.submitted_time
    }
}

impl Deref for TxInfo {
    type Target = ArcPoolTx;
    fn deref(&self) -> &Self::Target {
        &self.tx
    }
}
