use crate::service::Shared;
use fuel_core_types::{
    fuel_tx::TxId,
    fuel_types::BlockHeight,
};
use std::{
    collections::{
        BTreeMap,
        HashSet,
        VecDeque,
    },
    time::SystemTime,
};

pub(super) struct TransactionPruner {
    pub time_txs_submitted: Shared<VecDeque<(SystemTime, TxId)>>,
    pub height_expiration_txs: Shared<BTreeMap<BlockHeight, HashSet<TxId>>>,
    pub ttl_timer: tokio::time::Interval,
    pub txs_ttl: tokio::time::Duration,
}
