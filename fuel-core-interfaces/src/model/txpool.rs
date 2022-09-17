use crate::common::fuel_tx::Transaction;
use chrono::{
    DateTime,
    Utc,
};
use std::{
    ops::Deref,
    sync::Arc,
};

pub type ArcTx = Arc<Transaction>;

#[derive(Debug, Clone)]
pub struct TxInfo {
    tx: ArcTx,
    submitted_time: DateTime<Utc>,
}

impl TxInfo {
    pub fn new(tx: ArcTx) -> Self {
        Self {
            tx,
            submitted_time: Utc::now(),
        }
    }

    pub fn tx(&self) -> &ArcTx {
        &self.tx
    }

    pub fn submitted_time(&self) -> DateTime<Utc> {
        self.submitted_time
    }
}

impl Deref for TxInfo {
    type Target = ArcTx;
    fn deref(&self) -> &Self::Target {
        &self.tx
    }
}
