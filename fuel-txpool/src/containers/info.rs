use std::ops::Deref;

use crate::types::ArcTx;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone)]
pub struct TxInfo {
    tx: ArcTx,
    submited_time: DateTime<Utc>,
}

impl TxInfo {
    pub fn new(tx: ArcTx) -> Self {
        Self {
            tx,
            submited_time: Utc::now(),
        }
    }

    pub fn tx(&self) -> &ArcTx {
        &self.tx
    }

    pub fn submited_time(&self) -> DateTime<Utc> {
        self.submited_time
    }
}

impl Deref for TxInfo {
    type Target = ArcTx;
    fn deref(&self) -> &Self::Target {
        &self.tx
    }
}
