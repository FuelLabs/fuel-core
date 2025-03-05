use fuel_core_types::{
    fuel_tx::TxId,
    fuel_types::BlockHeight,
};
use std::{
    collections::{
        BTreeMap,
        VecDeque,
    },
    time::SystemTime,
};

pub(super) struct TransactionPruner {
    // TODO[RC]: Should pruner use the submission times from TxStatusManager?
    pub time_txs_submitted: VecDeque<(SystemTime, TxId)>,
    pub height_expiration_txs: BTreeMap<BlockHeight, Vec<TxId>>,
    pub ttl_timer: tokio::time::Interval,
    pub txs_ttl: tokio::time::Duration,
}
