use anyhow::anyhow;
use parking_lot::{
    Mutex,
    MutexGuard,
};
use std::{
    collections::{
        BTreeMap,
        HashMap,
    },
    sync::Arc,
    time::Duration,
};

use fuel_core_types::{
    fuel_tx::{
        Bytes32,
        TxId,
    },
    services::txpool::TransactionStatus,
    tai64::Tai64,
};

use crate::{
    error::Error,
    tx_status_stream::{
        TxStatusStream,
        TxUpdate,
    },
    update_sender::{
        MpscChannel,
        TxStatusChange,
    },
};

pub struct Data {
    timestamps: BTreeMap<Tai64, Vec<TxId>>,
    statuses: HashMap<TxId, TransactionStatus>,
}

impl Data {
    pub fn empty() -> Self {
        Self {
            timestamps: BTreeMap::new(),
            statuses: HashMap::new(),
        }
    }

    #[cfg(test)]
    fn assert_consistency(&self) {
        // There should be a timestamp entry for each registered tx id
        for status in self.statuses.keys() {
            assert!(self
                .timestamps
                .values()
                .any(|tx_ids| tx_ids.contains(status)));
        }

        // There should be a transaction with given id for each timestamp cache
        for tx_id in self.timestamps.values().flatten() {
            assert!(self.statuses.contains_key(tx_id));
        }

        // The count of transactions in both collections must match
        let tx_count = self.statuses.len();
        let tx_count_in_timestamps =
            self.timestamps.values().map(|tx_ids| tx_ids.len()).sum();
        assert_eq!(tx_count, tx_count_in_timestamps);
    }
}

#[derive(Clone)]
pub struct TxStatusManager {
    data: Arc<Mutex<Data>>,
    tx_status_change: TxStatusChange,
    ttl: u64,
}

impl TxStatusManager {
    pub fn new(tx_status_change: TxStatusChange, ttl: Duration) -> Self {
        Self {
            data: Arc::new(Mutex::new(Data::empty())),
            tx_status_change,
            ttl: ttl.as_secs(),
        }
    }

    fn prune_old_statuses(&self, data: &mut MutexGuard<Data>) {
        let timestamp = Tai64::now();
        let cutoff = timestamp - self.ttl;

        let old_ids = data
            .timestamps
            .range(..=cutoff)
            .flat_map(|(_, txid)| txid.iter().copied())
            .collect::<Vec<_>>();

        data.timestamps.retain(|&timestamp, _| timestamp > cutoff);
        for txid in old_ids {
            data.statuses.remove(&txid);
        }

        #[cfg(test)]
        data.assert_consistency();
    }

    fn add_new_status(
        &self,
        data: &mut MutexGuard<Data>,
        tx_id: TxId,
        tx_status: &TransactionStatus,
    ) {
        let timestamp = Tai64::now();
        data.timestamps.entry(timestamp).or_default().push(tx_id);
        data.statuses.insert(tx_id, tx_status.clone());
    }

    fn register_status(&self, tx_id: TxId, tx_status: &TransactionStatus) {
        let mut data = self.data.lock();
        self.prune_old_statuses(&mut data);
        self.add_new_status(&mut data, tx_id, tx_status);
    }

    pub fn status_update(&self, tx_id: TxId, tx_status: TransactionStatus) {
        tracing::debug!(%tx_id, ?tx_status, "new tx status");

        // TODO[RC]: Capacity checks? - Protected by TxPool capacity checks, except for the squeezed state. Maybe introduce some limit.
        // TODO[RC]: Purge old statuses? - Remove the status from the manager upon putting the status into storage.
        // TODO[RC]: Shall we store squeezed out variants as well?
        self.register_status(tx_id, &tx_status);

        match tx_status {
            TransactionStatus::Submitted { .. }
            | TransactionStatus::Success { .. }
            | TransactionStatus::SqueezedOut { .. }
            | TransactionStatus::Failure { .. } => (),
            // TODO[RC]: Handle these new variants
            TransactionStatus::PreConfirmationSuccess {
                tx_pointer: _,
                total_gas: _,
                total_fee: _,
                receipts: _,
                outputs: _,
            } => todo!(),
            TransactionStatus::PreConfirmationSqueezedOut { reason: _ } => {
                // TODO[RC]: Squeezed out should still have the tx_id, not just the reason
                todo!()
            }
            TransactionStatus::PreConfirmationFailure {
                tx_pointer: _,
                total_gas: _,
                total_fee: _,
                receipts: _,
                outputs: _,
                reason: _,
            } => todo!(),
        };

        self.tx_status_change
            .update_sender
            .send(&TxUpdate::new(tx_id, tx_status.into()));
    }

    pub fn status(&self, tx_id: &TxId) -> Option<TransactionStatus> {
        self.data.lock().statuses.get(tx_id).cloned()
    }

    /// Subscribe to status updates for a transaction.
    pub fn tx_update_subscribe(&self, tx_id: Bytes32) -> anyhow::Result<TxStatusStream> {
        self.tx_status_change
            .update_sender
            .try_subscribe::<MpscChannel>(tx_id)
            .ok_or(anyhow!("Maximum number of subscriptions reached"))
    }

    pub fn notify_skipped_txs(&self, tx_ids_and_reason: Vec<(Bytes32, String)>) {
        tx_ids_and_reason.into_iter().for_each(|(tx_id, reason)| {
            let error = Error::SkippedTransaction(reason);
            self.status_update(
                tx_id,
                TransactionStatus::SqueezedOut {
                    reason: error.to_string(),
                },
            );
        });
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use fuel_core_types::{
        fuel_tx::Bytes32,
        services::txpool::TransactionStatus,
        tai64::Tai64,
    };

    use crate::update_sender::TxStatusChange;

    use super::TxStatusManager;

    const DUMMY_STATUS: TransactionStatus = TransactionStatus::Submitted {
        timestamp: Tai64::UNIX_EPOCH,
    };

    const TTL: Duration = Duration::from_secs(4);

    fn assert_present(tx_status_manager: &TxStatusManager, tx_ids: Vec<Bytes32>) {
        for tx_id in tx_ids {
            assert!(
                tx_status_manager.status(&tx_id).is_some(),
                "tx_id {:?} should be present",
                tx_id
            );
        }
    }

    fn assert_missing(tx_status_manager: &TxStatusManager, tx_ids: Vec<Bytes32>) {
        for tx_id in tx_ids {
            assert!(
                tx_status_manager.status(&tx_id).is_none(),
                "tx_id {:?} should be missing",
                tx_id
            );
        }
    }

    #[test]
    fn simple_registration() {
        let tx_status_change = TxStatusChange::new(100, Duration::from_secs(360));
        let tx_status_manager = TxStatusManager::new(tx_status_change, TTL);

        let tx1_id = [1u8; 32].into();
        let tx2_id = [2u8; 32].into();
        let tx3_id = [3u8; 32].into();
        let tx4_id = [4u8; 32].into();

        // Register tx1 and tx2
        tx_status_manager.status_update(tx1_id, DUMMY_STATUS);
        tx_status_manager.status_update(tx2_id, DUMMY_STATUS);

        // Sleep for less than a TTL
        std::thread::sleep(Duration::from_secs(1));

        // Register tx3
        tx_status_manager.status_update(tx3_id, DUMMY_STATUS);

        // Sleep for less than a TTL
        std::thread::sleep(Duration::from_secs(1));

        // Register tx4
        tx_status_manager.status_update(tx4_id, DUMMY_STATUS);

        // All should be present
        assert_present(&tx_status_manager, vec![tx1_id, tx2_id, tx3_id, tx4_id]);
    }

    #[test]
    fn prunes_old_statuses() {
        let tx_status_change = TxStatusChange::new(100, Duration::from_secs(360));
        let tx_status_manager = TxStatusManager::new(tx_status_change, TTL);

        let tx1_id = [1u8; 32].into();
        let tx2_id = [2u8; 32].into();
        let tx3_id = [3u8; 32].into();
        let tx4_id = [4u8; 32].into();

        // Register tx1
        tx_status_manager.status_update(tx1_id, DUMMY_STATUS);
        assert_present(&tx_status_manager, vec![tx1_id]);

        // Move 2 second forward (half of TTL) and register tx2
        std::thread::sleep(TTL / 2);
        tx_status_manager.status_update(tx2_id, DUMMY_STATUS);

        // Both should be present, since TTL didn't pass yet
        assert_present(&tx_status_manager, vec![tx1_id, tx2_id]);

        // Move 3 second forward, for a total of 5s.
        // TTL = 4s, so tx1 should be pruned.
        std::thread::sleep(Duration::from_secs(3));

        // Trigger the pruning
        tx_status_manager.status_update(tx3_id, DUMMY_STATUS);

        // tx1 should be pruned, tx2 and tx3 should be present
        assert_missing(&tx_status_manager, vec![tx1_id]);
        assert_present(&tx_status_manager, vec![tx2_id, tx3_id]);

        // Move 2 second forward, for a total of 7s.
        // TTL = 4s, so tx2 should be pruned.
        std::thread::sleep(Duration::from_secs(2));

        // Trigger the pruning
        tx_status_manager.status_update(tx4_id, DUMMY_STATUS);

        // tx1 and tx2 should be pruned, tx3 and tx4 should be present
        assert_missing(&tx_status_manager, vec![tx1_id, tx2_id]);
        assert_present(&tx_status_manager, vec![tx3_id, tx4_id]);
    }

    #[test]
    fn prunes_multiple_old_statuses() {
        let tx_status_change = TxStatusChange::new(100, Duration::from_secs(360));
        let tx_status_manager = TxStatusManager::new(tx_status_change, TTL);

        let tx1_id = [1u8; 32].into();
        let tx2_id = [2u8; 32].into();
        let tx3_id = [3u8; 32].into();
        let tx4_id = [4u8; 32].into();
        let tx5_id = [5u8; 32].into();
        let tx6_id = [6u8; 32].into();

        // Register some transactions
        tx_status_manager.status_update(tx1_id, DUMMY_STATUS);
        tx_status_manager.status_update(tx2_id, DUMMY_STATUS);
        tx_status_manager.status_update(tx3_id, DUMMY_STATUS);

        // Sleep for less than TTL
        std::thread::sleep(Duration::from_secs(1));

        // Register some more transactions
        tx_status_manager.status_update(tx4_id, DUMMY_STATUS);
        tx_status_manager.status_update(tx5_id, DUMMY_STATUS);

        // Move beyond TTL
        std::thread::sleep(TTL);

        // Trigger the pruning
        tx_status_manager.status_update(tx6_id, DUMMY_STATUS);

        assert_missing(
            &tx_status_manager,
            vec![tx1_id, tx2_id, tx3_id, tx4_id, tx5_id],
        );
        assert_present(&tx_status_manager, vec![tx6_id]);
    }

    // TODO[RC]: Add test with the same TxId being updated
}
