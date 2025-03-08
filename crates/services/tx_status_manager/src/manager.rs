use anyhow::anyhow;
use std::{
    collections::{
        hash_map::Entry,
        HashMap,
        VecDeque,
    },
    time::{
        Duration,
        Instant,
    },
};

use fuel_core_types::{
    fuel_tx::{
        Bytes32,
        TxId,
    },
    services::txpool::TransactionStatus,
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
    pruning_queue: VecDeque<(Instant, TxId)>,
    statuses: HashMap<TxId, (Instant, TransactionStatus)>,
}

impl Data {
    pub fn empty() -> Self {
        Self {
            pruning_queue: VecDeque::new(),
            statuses: HashMap::new(),
        }
    }

    #[cfg(test)]
    fn assert_consistency(&self) {
        use std::collections::HashSet;
        // There should be a timestamp entry for each registered tx id
        for (status_tx_id, (status_time, _)) in self.statuses.iter() {
            assert!(self
                .pruning_queue
                .iter()
                .any(|(time, tx_id)| tx_id == status_tx_id && time == status_time));
        }

        // There should be a transaction with given id for each timestamp cache
        for (_, tx_id) in self.pruning_queue.iter() {
            assert!(self.statuses.contains_key(tx_id));
        }

        // The count of transactions in both collections must match
        let tx_count = self.statuses.len();
        let tx_count_from_queue = self
            .pruning_queue
            .iter()
            .map(|(_, tx_id)| *tx_id)
            .collect::<HashSet<_>>();
        assert_eq!(tx_count, tx_count_from_queue.len());
    }
}

pub struct TxStatusManager {
    data: Data,
    tx_status_change: TxStatusChange,
    ttl: Duration,
}

impl TxStatusManager {
    pub fn new(tx_status_change: TxStatusChange, ttl: Duration) -> Self {
        Self {
            data: Data::empty(),
            tx_status_change,
            ttl,
        }
    }

    fn prune_old_statuses(&mut self) {
        let now = Instant::now();

        while let Some((past, _)) = self.data.pruning_queue.back() {
            let duration = now.duration_since(*past);

            if duration < self.ttl {
                break;
            }

            let (past, tx_id) = self
                .data
                .pruning_queue
                .pop_back()
                .expect("Queue is not empty, checked above; qed");

            let entry = self.data.statuses.entry(tx_id);
            match entry {
                Entry::Occupied(entry) => {
                    // One transaction can have multiple statuses during its lifetime.
                    // It can have `Submitted`, `PreConfirmationSuccess`, `Success`, for example.
                    // When we insert each of new status, we also update a timestamp when it was
                    // inserted.
                    //
                    // Prune queue can have multiple entries for the same transaction.
                    // We only need to remove status from the cache if it is the last status
                    // for the transaction(in other words timestamp from queue matches
                    // timestamp from the cache).
                    if entry.get().0 == past {
                        entry.remove();
                    }
                }
                Entry::Vacant(_) => {
                    // If it was already removed, then we do nothing
                }
            }
        }

        #[cfg(test)]
        self.data.assert_consistency();
    }

    fn add_new_status(&mut self, tx_id: TxId, tx_status: TransactionStatus) {
        let now = Instant::now();
        self.data.pruning_queue.push_front((now, tx_id));
        self.data.statuses.insert(tx_id, (now, tx_status));
    }

    fn register_status(&mut self, tx_id: TxId, tx_status: TransactionStatus) {
        self.prune_old_statuses();
        self.add_new_status(tx_id, tx_status);
    }

    pub fn status_update(&mut self, tx_id: TxId, tx_status: TransactionStatus) {
        tracing::debug!(%tx_id, ?tx_status, "new tx status");

        // TODO[RC]: Capacity checks? - Protected by TxPool capacity checks, except for the squeezed state. Maybe introduce some limit.
        // TODO[RC]: Shall we store squeezed out variants as well?
        self.register_status(tx_id, tx_status.clone());

        match &tx_status {
            TransactionStatus::Submitted(_)
            | TransactionStatus::Success(_)
            | TransactionStatus::SqueezedOut(_)
            | TransactionStatus::Failure(_) => (),
            // TODO[RC]: Handle these new variants
            TransactionStatus::PreConfirmationSuccess(_) => todo!(),
            TransactionStatus::PreConfirmationSqueezedOut(_) => {
                // TODO[RC]: Squeezed out should still have the tx_id, not just the reason
                todo!()
            }
            TransactionStatus::PreConfirmationFailure(_) => todo!(),
        };

        self.tx_status_change
            .update_sender
            .send(TxUpdate::new(tx_id, tx_status.into()));
    }

    pub fn status(&self, tx_id: &TxId) -> Option<TransactionStatus> {
        self.data
            .statuses
            .get(tx_id)
            .map(|(_, status)| status.clone())
    }

    /// Subscribe to status updates for a transaction.
    pub fn tx_update_subscribe(&self, tx_id: Bytes32) -> anyhow::Result<TxStatusStream> {
        self.tx_status_change
            .update_sender
            .try_subscribe::<MpscChannel>(tx_id)
            .ok_or(anyhow!("Maximum number of subscriptions reached"))
    }

    pub fn notify_skipped_txs(&mut self, tx_ids_and_reason: Vec<(Bytes32, String)>) {
        tx_ids_and_reason.into_iter().for_each(|(tx_id, reason)| {
            let error = Error::SkippedTransaction(reason);
            self.status_update(tx_id, TransactionStatus::squeezed_out(error.to_string()));
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

    use super::TxStatusManager;

    fn status_1() -> TransactionStatus {
        TransactionStatus::submitted(Tai64::UNIX_EPOCH)
    }

    const TTL: Duration = Duration::from_secs(4);

    fn assert_presence(tx_status_manager: &TxStatusManager, tx_ids: Vec<Bytes32>) {
        for tx_id in tx_ids {
            assert!(
                tx_status_manager.status(&tx_id).is_some(),
                "tx_id {:?} should be present",
                tx_id
            );
        }
    }

    fn assert_presence_with_status(
        tx_status_manager: &TxStatusManager,
        txs: Vec<(Bytes32, TransactionStatus)>,
    ) {
        for (tx_id, status) in txs {
            assert!(
                tx_status_manager.status(&tx_id) == Some(status),
                "tx_id {:?} should be present with correct status",
                tx_id
            );
        }
    }

    fn assert_absence(tx_status_manager: &TxStatusManager, tx_ids: Vec<Bytes32>) {
        for tx_id in tx_ids {
            assert!(
                tx_status_manager.status(&tx_id).is_none(),
                "tx_id {:?} should be missing",
                tx_id
            );
        }
    }

    mod equal_ids {
        use std::time::Duration;

        use crate::{
            update_sender::TxStatusChange,
            TxStatusManager,
        };

        use super::{
            assert_absence,
            assert_presence,
            status_1,
            TTL,
        };

        #[test]
        fn simple_registration() {
            let tx_status_change = TxStatusChange::new(100, Duration::from_secs(360));
            let mut tx_status_manager = TxStatusManager::new(tx_status_change, TTL);

            let tx1_id = [1u8; 32].into();
            let tx2_id = [2u8; 32].into();
            let tx3_id = [3u8; 32].into();
            let tx4_id = [4u8; 32].into();

            // Register tx1 and tx2
            tx_status_manager.status_update(tx1_id, status_1());
            tx_status_manager.status_update(tx2_id, status_1());

            // Sleep for less than a TTL
            std::thread::sleep(Duration::from_secs(1));

            // Register tx3
            tx_status_manager.status_update(tx3_id, status_1());

            // Sleep for less than a TTL
            std::thread::sleep(Duration::from_secs(1));

            // Register tx4
            tx_status_manager.status_update(tx4_id, status_1());

            // All should be present
            assert_presence(&tx_status_manager, vec![tx1_id, tx2_id, tx3_id, tx4_id]);
        }

        #[test]
        fn prunes_old_statuses() {
            let tx_status_change = TxStatusChange::new(100, Duration::from_secs(360));
            let mut tx_status_manager = TxStatusManager::new(tx_status_change, TTL);

            let tx1_id = [1u8; 32].into();
            let tx2_id = [2u8; 32].into();
            let tx3_id = [3u8; 32].into();
            let tx4_id = [4u8; 32].into();

            // Register tx1
            tx_status_manager.status_update(tx1_id, status_1());
            assert_presence(&tx_status_manager, vec![tx1_id]);

            // Move 2 second forward (half of TTL) and register tx2
            std::thread::sleep(TTL / 2);
            tx_status_manager.status_update(tx2_id, status_1());

            // Both should be present, since TTL didn't pass yet
            assert_presence(&tx_status_manager, vec![tx1_id, tx2_id]);

            // Move 3 second forward, for a total of 5s.
            // TTL = 4s, so tx1 should be pruned.
            std::thread::sleep(Duration::from_secs(3));

            // Trigger the pruning
            tx_status_manager.status_update(tx3_id, status_1());

            // tx1 should be pruned, tx2 and tx3 should be present
            assert_absence(&tx_status_manager, vec![tx1_id]);
            assert_presence(&tx_status_manager, vec![tx2_id, tx3_id]);

            // Move 2 second forward, for a total of 7s.
            // TTL = 4s, so tx2 should be pruned.
            std::thread::sleep(Duration::from_secs(2));

            // Trigger the pruning
            tx_status_manager.status_update(tx4_id, status_1());

            // tx1 and tx2 should be pruned, tx3 and tx4 should be present
            assert_absence(&tx_status_manager, vec![tx1_id, tx2_id]);
            assert_presence(&tx_status_manager, vec![tx3_id, tx4_id]);
        }

        #[test]
        fn prunes_multiple_old_statuses() {
            let tx_status_change = TxStatusChange::new(100, Duration::from_secs(360));
            let mut tx_status_manager = TxStatusManager::new(tx_status_change, TTL);

            let tx1_id = [1u8; 32].into();
            let tx2_id = [2u8; 32].into();
            let tx3_id = [3u8; 32].into();
            let tx4_id = [4u8; 32].into();
            let tx5_id = [5u8; 32].into();
            let tx6_id = [6u8; 32].into();

            // Register some transactions
            tx_status_manager.status_update(tx1_id, status_1());
            tx_status_manager.status_update(tx2_id, status_1());
            tx_status_manager.status_update(tx3_id, status_1());

            // Sleep for less than TTL
            std::thread::sleep(Duration::from_secs(1));

            // Register some more transactions
            tx_status_manager.status_update(tx4_id, status_1());
            tx_status_manager.status_update(tx5_id, status_1());

            // Move beyond TTL
            std::thread::sleep(TTL);

            // Trigger the pruning
            tx_status_manager.status_update(tx6_id, status_1());

            // All but the last one should be pruned.
            assert_absence(
                &tx_status_manager,
                vec![tx1_id, tx2_id, tx3_id, tx4_id, tx5_id],
            );
            assert_presence(&tx_status_manager, vec![tx6_id]);
        }
    }

    mod distinct_ids {
        use fuel_core_types::services::txpool::TransactionStatus;

        use crate::{
            update_sender::TxStatusChange,
            TxStatusManager,
        };

        use super::{
            assert_absence,
            assert_presence_with_status,
            status_1,
            Duration,
            TTL,
        };

        #[test]
        fn simple_registration() {
            let status_2: TransactionStatus =
                TransactionStatus::squeezed_out("fishy tx".to_string());

            let tx_status_change = TxStatusChange::new(100, Duration::from_secs(360));
            let mut tx_status_manager = TxStatusManager::new(tx_status_change, TTL);

            let tx1_id = [1u8; 32].into();
            let tx2_id = [2u8; 32].into();

            // Register tx1 and tx2
            tx_status_manager.status_update(tx1_id, status_1());
            tx_status_manager.status_update(tx1_id, status_2.clone());

            // Sleep for less than a TTL
            std::thread::sleep(Duration::from_secs(1));

            // Register tx2
            tx_status_manager.status_update(tx2_id, status_1());

            // All should be present
            assert_presence_with_status(
                &tx_status_manager,
                vec![(tx1_id, status_2), (tx2_id, status_1())],
            );
        }

        #[test]
        fn prunes_old_statuses() {
            let status_2: TransactionStatus =
                TransactionStatus::squeezed_out("fishy tx".to_string());

            let tx_status_change = TxStatusChange::new(100, Duration::from_secs(360));
            let mut tx_status_manager = TxStatusManager::new(tx_status_change, TTL);

            let tx1_id = [1u8; 32].into();
            let tx2_id = [2u8; 32].into();
            let tx3_id = [3u8; 32].into();
            let tx4_id = [4u8; 32].into();

            // Register tx1 and tx3
            tx_status_manager.status_update(tx1_id, status_1());
            tx_status_manager.status_update(tx3_id, status_1());

            // Move 2 second forward (half of TTL), register tx2
            // and update status of tx1
            std::thread::sleep(TTL / 2);
            tx_status_manager.status_update(tx2_id, status_1());
            tx_status_manager.status_update(tx1_id, status_2.clone());

            // All should be present, since TTL didn't pass yet
            assert_presence_with_status(
                &tx_status_manager,
                vec![
                    (tx1_id, status_2.clone()),
                    (tx2_id, status_1()),
                    (tx3_id, status_1()),
                ],
            );

            // Move 3 second forward, for a total of 5s.
            // TTL = 4s, so tx1 should be pruned.
            std::thread::sleep(Duration::from_secs(3));

            // Trigger the pruning
            tx_status_manager.status_update(tx4_id, status_1());

            // Only tx3 should be pruned since it's in the manager
            // since the beginning. tx2 was registered later
            // and the status (and timestamp) of tx1 was
            // also update later.
            assert_presence_with_status(
                &tx_status_manager,
                vec![(tx1_id, status_2), (tx2_id, status_1())],
            );
        }

        #[test]
        fn prunes_multiple_old_statuses() {
            let status_2: TransactionStatus =
                TransactionStatus::squeezed_out("fishy tx".to_string());

            let tx_status_change = TxStatusChange::new(100, Duration::from_secs(360));
            let mut tx_status_manager = TxStatusManager::new(tx_status_change, TTL);

            let tx1_id = [1u8; 32].into();
            let tx2_id = [2u8; 32].into();
            let tx3_id = [3u8; 32].into();
            let tx4_id = [4u8; 32].into();
            let tx5_id = [5u8; 32].into();
            let tx6_id = [6u8; 32].into();

            // Register some transactions
            tx_status_manager.status_update(tx1_id, status_1());
            tx_status_manager.status_update(tx2_id, status_1());
            tx_status_manager.status_update(tx3_id, status_1());

            // Sleep for less than TTL
            std::thread::sleep(Duration::from_secs(1));

            // Register some more transactions and update
            // some old statuses
            tx_status_manager.status_update(tx4_id, status_1());
            tx_status_manager.status_update(tx5_id, status_1());
            tx_status_manager.status_update(tx1_id, status_2.clone());
            tx_status_manager.status_update(tx2_id, status_2.clone());

            // Move beyond TTL
            std::thread::sleep(TTL);

            // Trigger the pruning
            tx_status_manager.status_update(tx6_id, status_1());

            // All but the last one should be pruned.
            assert_absence(
                &tx_status_manager,
                vec![tx1_id, tx2_id, tx3_id, tx4_id, tx5_id],
            );
            assert_presence_with_status(&tx_status_manager, vec![(tx6_id, status_1())]);
        }

        #[test]
        fn status_preserved_in_cache_when_first_status_expires() {
            let status_2: TransactionStatus =
                TransactionStatus::squeezed_out("fishy tx".to_string());

            let tx_status_change = TxStatusChange::new(100, Duration::from_secs(360));
            let mut tx_status_manager = TxStatusManager::new(tx_status_change, TTL);

            let tx1_id = [1u8; 32].into();

            // Register tx1 and remember it's timestamp
            tx_status_manager.status_update(tx1_id, status_1());

            // Sleep for less than a TTL
            std::thread::sleep(Duration::from_secs(2));

            // Given
            // Update tx1 status, the timestamp cache should get updated.
            tx_status_manager.status_update(tx1_id, status_2);

            // When
            // Sleep for TTL, it should expire the first status
            std::thread::sleep(TTL);

            // Then
            let status = tx_status_manager.status(&tx1_id);
            assert!(status.is_some());
        }

        // TODO[RC]: Optimization - update the tests to not rely on systemtime, inject
        // timestamps instead. This will make it possible to write a efficient proptest.
    }
}
