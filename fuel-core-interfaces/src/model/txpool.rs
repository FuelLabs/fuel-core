use fuel_tx::Transaction;
use std::ops::Deref;
use std::sync::Arc;
use time::OffsetDateTime;

pub type ArcTx = Arc<Transaction>;

#[derive(Debug, Clone)]
pub struct TxInfo {
    tx: ArcTx,
    submitted_time: OffsetDateTime,
}

impl TxInfo {
    pub fn new(tx: ArcTx) -> Self {
        Self {
            tx,
            submitted_time: OffsetDateTime::now_utc(),
        }
    }

    pub fn tx(&self) -> &ArcTx {
        &self.tx
    }

    pub fn submitted_time(&self) -> OffsetDateTime {
        self.submitted_time
    }
}

impl Deref for TxInfo {
    type Target = ArcTx;
    fn deref(&self) -> &Self::Target {
        &self.tx
    }
}
