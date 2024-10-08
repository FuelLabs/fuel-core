use crate::service::Shared;
use fuel_core_types::fuel_tx::TxId;
use std::{
    collections::VecDeque,
    time::SystemTime,
};

pub(super) struct TransactionPruner {
    pub time_txs_submitted: Shared<VecDeque<(SystemTime, TxId)>>,
    pub ttl_timer: tokio::time::Interval,
    pub txs_ttl: tokio::time::Duration,
}
