use anyhow::anyhow;
use parking_lot::{
    Mutex,
    MutexGuard,
};
use std::{
    collections::{
        BTreeMap,
        BTreeSet,
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
    timestamps: BTreeMap<Tai64, BTreeSet<TxId>>,
    statuses: HashMap<TxId, (TransactionStatus, Tai64)>,
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

pub trait TimeProvider {
    fn now(&self) -> Tai64 {
        Tai64::now()
    }

    #[cfg(test)]
    fn sleep(&mut self, secs: i64) {
        std::thread::sleep(Duration::from_secs(secs as u64));
    }
}

#[derive(Clone)]
pub struct TxStatusManager<Time>
where
    Time: TimeProvider,
{
    data: Arc<Mutex<Data>>,
    tx_status_change: TxStatusChange,
    ttl: u64,
    time_provider: Time,
}

impl<Time> TxStatusManager<Time>
where
    Time: TimeProvider,
{
    pub fn new(
        tx_status_change: TxStatusChange,
        ttl: Duration,
        time_provider: Time,
    ) -> Self {
        Self {
            data: Arc::new(Mutex::new(Data::empty())),
            tx_status_change,
            ttl: ttl.as_secs(),
            time_provider,
        }
    }

    #[cfg(test)]
    fn inner_data(&self) -> MutexGuard<Data> {
        self.data.lock()
    }

    #[cfg(test)]
    fn advance_time(&mut self, secs: i64) {
        self.time_provider.sleep(secs);
    }

    #[cfg(test)]
    fn now(&mut self) -> Tai64 {
        self.time_provider.now()
    }

    fn prune_old_statuses(&self, data: &mut MutexGuard<Data>) {
        let timestamp = self.time_provider.now();

        // If timestamp is longer than the entire possible timespan we can not
        // do much about it anyway.
        #[allow(clippy::arithmetic_side_effects)]
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
        let timestamp = self.time_provider.now();

        if let Some((_, prev_timestamp)) = data.statuses.get(&tx_id) {
            if timestamp != *prev_timestamp {
                let prev_timestamp_clone = *prev_timestamp;
                if let Some(timestamps) = data.timestamps.get_mut(&prev_timestamp_clone) {
                    timestamps.remove(&tx_id);
                    if timestamps.is_empty() {
                        data.timestamps.remove(&prev_timestamp_clone);
                    }
                } else {
                    tracing::error!(%tx_id, "status manager inconsistency")
                }
            }
        }

        data.timestamps.entry(timestamp).or_default().insert(tx_id);
        data.statuses.insert(tx_id, (tx_status.clone(), timestamp));
    }

    fn register_status(&self, tx_id: TxId, tx_status: &TransactionStatus) {
        let mut data = self.data.lock();
        self.prune_old_statuses(&mut data);
        self.add_new_status(&mut data, tx_id, tx_status);
    }

    pub fn status_update(&self, tx_id: TxId, tx_status: TransactionStatus) {
        tracing::debug!(%tx_id, ?tx_status, "new tx status");

        // TODO[RC]: Capacity checks? - Protected by TxPool capacity checks, except for the squeezed state. Maybe introduce some limit.
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
        self.data
            .lock()
            .statuses
            .get(tx_id)
            .map(|(status, _)| status)
            .cloned()
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

    use super::{
        TimeProvider,
        TxStatusManager,
    };

    const STATUS_1: TransactionStatus = TransactionStatus::Submitted {
        timestamp: Tai64::UNIX_EPOCH,
    };

    const TTL: Duration = Duration::from_secs(4);

    fn assert_presence<Time>(
        tx_status_manager: &TxStatusManager<Time>,
        tx_ids: Vec<Bytes32>,
    ) where
        Time: TimeProvider,
    {
        for tx_id in tx_ids {
            assert!(
                tx_status_manager.status(&tx_id).is_some(),
                "tx_id {:?} should be present",
                tx_id
            );
        }
    }

    fn assert_presence_with_status<Time>(
        tx_status_manager: &TxStatusManager<Time>,
        txs: Vec<(Bytes32, TransactionStatus)>,
    ) where
        Time: TimeProvider,
    {
        for (tx_id, status) in txs {
            assert!(
                tx_status_manager.status(&tx_id) == Some(status),
                "tx_id {:?} should be present with correct status",
                tx_id
            );
        }
    }

    fn assert_absence<Time>(
        tx_status_manager: &TxStatusManager<Time>,
        tx_ids: Vec<Bytes32>,
    ) where
        Time: TimeProvider,
    {
        for tx_id in tx_ids {
            assert!(
                tx_status_manager.status(&tx_id).is_none(),
                "tx_id {:?} should be missing",
                tx_id
            );
        }
    }

    struct TestTimeProvider {
        now: i64,
    }

    impl TestTimeProvider {
        fn new() -> Self {
            Self { now: 0 }
        }
    }

    impl TimeProvider for TestTimeProvider {
        fn now(&self) -> Tai64 {
            Tai64::from_unix(self.now)
        }

        fn sleep(&mut self, secs: i64) {
            self.now = self.now.saturating_add(secs);
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
            TestTimeProvider,
            STATUS_1,
            TTL,
        };

        #[test]
        fn simple_registration() {
            let test_time_provider = TestTimeProvider::new();
            let tx_status_change = TxStatusChange::new(100, Duration::from_secs(360));
            let mut tx_status_manager =
                TxStatusManager::new(tx_status_change, TTL, test_time_provider);

            let tx1_id = [1u8; 32].into();
            let tx2_id = [2u8; 32].into();
            let tx3_id = [3u8; 32].into();
            let tx4_id = [4u8; 32].into();

            // Register tx1 and tx2
            tx_status_manager.status_update(tx1_id, STATUS_1);
            tx_status_manager.status_update(tx2_id, STATUS_1);

            // Sleep for less than a TTL
            tx_status_manager.advance_time(1);

            // Register tx3
            tx_status_manager.status_update(tx3_id, STATUS_1);

            // Sleep for less than a TTL
            tx_status_manager.advance_time(1);

            // Register tx4
            tx_status_manager.status_update(tx4_id, STATUS_1);

            // All should be present
            assert_presence(&tx_status_manager, vec![tx1_id, tx2_id, tx3_id, tx4_id]);
        }

        #[test]
        fn prunes_old_statuses() {
            let test_time_provider = TestTimeProvider::new();
            let tx_status_change = TxStatusChange::new(100, Duration::from_secs(360));
            let mut tx_status_manager =
                TxStatusManager::new(tx_status_change, TTL, test_time_provider);

            let tx1_id = [1u8; 32].into();
            let tx2_id = [2u8; 32].into();
            let tx3_id = [3u8; 32].into();
            let tx4_id = [4u8; 32].into();

            // Register tx1
            tx_status_manager.status_update(tx1_id, STATUS_1);
            assert_presence(&tx_status_manager, vec![tx1_id]);

            // Move 2 second forward (half of TTL) and register tx2
            tx_status_manager.advance_time((TTL / 2).as_secs() as i64);
            tx_status_manager.status_update(tx2_id, STATUS_1);

            // Both should be present, since TTL didn't pass yet
            assert_presence(&tx_status_manager, vec![tx1_id, tx2_id]);

            // Move 3 second forward, for a total of 5s.
            // TTL = 4s, so tx1 should be pruned.
            tx_status_manager.advance_time(3);

            // Trigger the pruning
            tx_status_manager.status_update(tx3_id, STATUS_1);

            // tx1 should be pruned, tx2 and tx3 should be present
            assert_absence(&tx_status_manager, vec![tx1_id]);
            assert_presence(&tx_status_manager, vec![tx2_id, tx3_id]);

            // Move 2 second forward, for a total of 7s.
            // TTL = 4s, so tx2 should be pruned.
            tx_status_manager.advance_time(2);

            // Trigger the pruning
            tx_status_manager.status_update(tx4_id, STATUS_1);

            // tx1 and tx2 should be pruned, tx3 and tx4 should be present
            assert_absence(&tx_status_manager, vec![tx1_id, tx2_id]);
            assert_presence(&tx_status_manager, vec![tx3_id, tx4_id]);
        }

        #[test]
        fn prunes_multiple_old_statuses() {
            let test_time_provider = TestTimeProvider::new();
            let tx_status_change = TxStatusChange::new(100, Duration::from_secs(360));
            let mut tx_status_manager =
                TxStatusManager::new(tx_status_change, TTL, test_time_provider);

            let tx1_id = [1u8; 32].into();
            let tx2_id = [2u8; 32].into();
            let tx3_id = [3u8; 32].into();
            let tx4_id = [4u8; 32].into();
            let tx5_id = [5u8; 32].into();
            let tx6_id = [6u8; 32].into();

            // Register some transactions
            tx_status_manager.status_update(tx1_id, STATUS_1);
            tx_status_manager.status_update(tx2_id, STATUS_1);
            tx_status_manager.status_update(tx3_id, STATUS_1);

            // Sleep for less than TTL
            tx_status_manager.advance_time(1);

            // Register some more transactions
            tx_status_manager.status_update(tx4_id, STATUS_1);
            tx_status_manager.status_update(tx5_id, STATUS_1);

            // Move beyond TTL
            tx_status_manager.advance_time(TTL.as_secs() as i64);

            // Trigger the pruning
            tx_status_manager.status_update(tx6_id, STATUS_1);

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
            Duration,
            TestTimeProvider,
            STATUS_1,
            TTL,
        };

        #[test]
        fn simple_registration() {
            let test_time_provider = TestTimeProvider::new();
            let status_2: TransactionStatus = TransactionStatus::SqueezedOut {
                reason: "fishy tx".to_string(),
            };

            let tx_status_change = TxStatusChange::new(100, Duration::from_secs(360));
            let mut tx_status_manager =
                TxStatusManager::new(tx_status_change, TTL, test_time_provider);

            let tx1_id = [1u8; 32].into();
            let tx2_id = [2u8; 32].into();

            // Register tx1 and tx2
            tx_status_manager.status_update(tx1_id, STATUS_1);
            tx_status_manager.status_update(tx1_id, status_2.clone());

            // Sleep for less than a TTL
            tx_status_manager.advance_time(1);

            // Register tx2
            tx_status_manager.status_update(tx2_id, STATUS_1);

            // All should be present
            assert_presence_with_status(
                &tx_status_manager,
                vec![(tx1_id, status_2), (tx2_id, STATUS_1)],
            );
        }

        #[test]
        fn prunes_old_statuses() {
            let test_time_provider = TestTimeProvider::new();
            let status_2: TransactionStatus = TransactionStatus::SqueezedOut {
                reason: "fishy tx".to_string(),
            };

            let tx_status_change = TxStatusChange::new(100, Duration::from_secs(360));
            let mut tx_status_manager =
                TxStatusManager::new(tx_status_change, TTL, test_time_provider);

            let tx1_id = [1u8; 32].into();
            let tx2_id = [2u8; 32].into();
            let tx3_id = [3u8; 32].into();
            let tx4_id = [4u8; 32].into();

            // Register tx1 and tx3
            tx_status_manager.status_update(tx1_id, STATUS_1);
            tx_status_manager.status_update(tx3_id, STATUS_1);

            // Move 2 second forward (half of TTL), register tx2
            // and update status of tx1
            tx_status_manager.advance_time((TTL / 2).as_secs() as i64);
            tx_status_manager.status_update(tx2_id, STATUS_1);
            tx_status_manager.status_update(tx1_id, status_2.clone());

            // All should be present, since TTL didn't pass yet
            assert_presence_with_status(
                &tx_status_manager,
                vec![
                    (tx1_id, status_2.clone()),
                    (tx2_id, STATUS_1),
                    (tx3_id, STATUS_1),
                ],
            );

            // Move 3 second forward, for a total of 5s.
            // TTL = 4s, so tx1 should be pruned.
            tx_status_manager.advance_time(3);

            // Trigger the pruning
            tx_status_manager.status_update(tx4_id, STATUS_1);

            // Only tx3 should be pruned since it's in the manager
            // since the beginning. tx2 was registered later
            // and the status (and timestamp) of tx1 was
            // also update later.
            assert_presence_with_status(
                &tx_status_manager,
                vec![(tx1_id, status_2), (tx2_id, STATUS_1)],
            );
        }

        #[test]
        fn prunes_multiple_old_statuses() {
            let test_time_provider = TestTimeProvider::new();
            let status_2: TransactionStatus = TransactionStatus::SqueezedOut {
                reason: "fishy tx".to_string(),
            };

            let tx_status_change = TxStatusChange::new(100, Duration::from_secs(360));
            let mut tx_status_manager =
                TxStatusManager::new(tx_status_change, TTL, test_time_provider);

            let tx1_id = [1u8; 32].into();
            let tx2_id = [2u8; 32].into();
            let tx3_id = [3u8; 32].into();
            let tx4_id = [4u8; 32].into();
            let tx5_id = [5u8; 32].into();
            let tx6_id = [6u8; 32].into();

            // Register some transactions
            tx_status_manager.status_update(tx1_id, STATUS_1);
            tx_status_manager.status_update(tx2_id, STATUS_1);
            tx_status_manager.status_update(tx3_id, STATUS_1);

            // Sleep for less than TTL
            tx_status_manager.advance_time(1);

            // Register some more transactions and update
            // some old statuses
            tx_status_manager.status_update(tx4_id, STATUS_1);
            tx_status_manager.status_update(tx5_id, STATUS_1);
            tx_status_manager.status_update(tx1_id, status_2.clone());
            tx_status_manager.status_update(tx2_id, status_2.clone());

            // Move beyond TTL
            tx_status_manager.advance_time(TTL.as_secs() as i64);

            // Trigger the pruning
            tx_status_manager.status_update(tx6_id, STATUS_1);

            // All but the last one should be pruned.
            assert_absence(
                &tx_status_manager,
                vec![tx1_id, tx2_id, tx3_id, tx4_id, tx5_id],
            );
            assert_presence_with_status(&tx_status_manager, vec![(tx6_id, STATUS_1)]);
        }

        #[test]
        fn removes_empty_map_from_cache() {
            let status_2: TransactionStatus = TransactionStatus::SqueezedOut {
                reason: "fishy tx".to_string(),
            };

            let test_time_provider = TestTimeProvider::new();
            let tx_status_change = TxStatusChange::new(100, Duration::from_secs(360));
            let mut tx_status_manager =
                TxStatusManager::new(tx_status_change, TTL, test_time_provider);

            let tx1_id = [1u8; 32].into();

            // Register tx1 and remember it's timestamp
            tx_status_manager.status_update(tx1_id, STATUS_1);
            let timestamp = {
                let data = tx_status_manager.inner_data();
                data.timestamps.keys().copied().next().unwrap()
            };

            // Sleep for less than a TTL
            tx_status_manager.advance_time(2);

            // Update tx1 status, the timestamp cache should get updated.
            tx_status_manager.status_update(tx1_id, status_2);

            // Check that there is no stray cache entry for the original timestamp.
            {
                let data = tx_status_manager.inner_data();
                let empty_timestamp_cache_exists =
                    data.timestamps.contains_key(&timestamp);
                assert!(!empty_timestamp_cache_exists);
            }
        }

        #[test]
        fn status_preserved_in_cache_when_first_status_expires() {
            let status_2: TransactionStatus = TransactionStatus::SqueezedOut {
                reason: "fishy tx".to_string(),
            };

            let test_time_provider = TestTimeProvider::new();
            let tx_status_change = TxStatusChange::new(100, Duration::from_secs(360));
            let mut tx_status_manager =
                TxStatusManager::new(tx_status_change, TTL, test_time_provider);

            let tx1_id = [1u8; 32].into();
            let tx2_id = [2u8; 32].into();

            // Register tx1 and remember it's timestamp
            tx_status_manager.status_update(tx1_id, STATUS_1);

            // Sleep for less than a TTL
            tx_status_manager.advance_time(2);

            // Given
            // Update tx1 status, the timestamp cache should get updated.
            tx_status_manager.status_update(tx1_id, status_2.clone());

            // When
            // Sleep for TTL, it should expire the first status
            tx_status_manager.advance_time(1000_i64);

            // Trigger pruning
            tx_status_manager.status_update(tx2_id, status_2);

            // Then
            let status = tx_status_manager.status(&tx1_id);
            assert!(status.is_none());
        }
    }

    use proptest::prelude::*;
    use std::collections::HashMap;

    const TX_ID_POOL_SIZE: usize = 20;
    const MIN_ACTIONS: usize = 50;
    const MAX_ACTIONS: usize = 1000;
    const MIN_TTL: u64 = 10;
    const MAX_TTL: u64 = 360;

    #[derive(Debug, Clone)]
    enum Action {
        UpdateStatus { tx_id_index: usize },
        AdvanceTime { seconds: u64 },
    }

    // How to select an ID from the pool
    fn tx_id_index_strategy(pool_size: usize) -> impl Strategy<Value = usize> {
        0..pool_size
    }

    // Possible values for TTL
    fn ttl_strategy(min_ttl: u64, max_ttl: u64) -> impl Strategy<Value = Duration> {
        (min_ttl..=max_ttl).prop_map(Duration::from_secs)
    }

    // Custom strategy to generate a sequence of actions
    fn actions_strategy(
        min_actions: usize,
        max_actions: usize,
    ) -> impl Strategy<Value = Vec<Action>> {
        let update_status_strategy = (tx_id_index_strategy(TX_ID_POOL_SIZE))
            .prop_map(|tx_id_index| Action::UpdateStatus { tx_id_index });

        let advance_time_strategy =
            (1..=MAX_TTL / 2).prop_map(|seconds| Action::AdvanceTime { seconds });

        prop::collection::vec(
            prop_oneof![update_status_strategy, advance_time_strategy],
            min_actions..max_actions,
        )
    }

    // Generate a pool of unique transaction IDs
    fn generate_tx_id_pool() -> Vec<[u8; 32]> {
        (0..TX_ID_POOL_SIZE)
            .map(|i| {
                let mut tx_id = [0u8; 32];
                tx_id[0..8].copy_from_slice(&(i as u64).to_le_bytes());
                tx_id
            })
            .collect()
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        #[allow(clippy::arithmetic_side_effects)]
        fn test_status_manager_pruning(
            ttl in ttl_strategy(MIN_TTL, MAX_TTL),
            actions in actions_strategy(MIN_ACTIONS, MAX_ACTIONS)
        ) {
            let tx_status_change = TxStatusChange::new(100, Duration::from_secs(360));

            let test_time_provider = TestTimeProvider::new();
            let mut tx_status_manager = TxStatusManager::new(tx_status_change, ttl, test_time_provider);

            let tx_id_pool = generate_tx_id_pool();

            // This will be used to track when each txid was updated so that
            // we can do the final assert against the TTL.
            let mut update_times = HashMap::new();

            // Simulate flow of time and transaction updates
            for action in actions {
                match action {
                    Action::UpdateStatus { tx_id_index } => {
                        let tx_id = tx_id_pool[tx_id_index];
                        tx_status_manager.status_update(tx_id.into(), STATUS_1);
                        update_times.insert(tx_id, tx_status_manager.now());

                    },
                    Action::AdvanceTime { seconds } => {
                        tx_status_manager.advance_time(seconds as i64);
                    }
                }
            }

            // Trigger the final pruning, making sure we use ID that is not
            // in the pool
            let final_tx_id = {
                let marker: u64 = 0xDEADBEEF;
                let mut id = [0u8; 32];
                id[0..8].copy_from_slice(&marker.to_le_bytes());
                id
            };
            assert!(!tx_id_pool.contains(&final_tx_id));
            tx_status_manager.status_update(final_tx_id.into(), STATUS_1);

            // Verify that only recent transactions are present
            let (recent_tx_ids, not_recent_tx_ids): (Vec<_>, Vec<_>) = update_times
                .iter()
                .partition(|(_, &time)| time + ttl.as_secs() > tx_status_manager.now());
            assert_presence(&tx_status_manager, recent_tx_ids.into_iter().map(|(tx_id, _)| (*tx_id).into()).collect());
            assert_absence(&tx_status_manager, not_recent_tx_ids.into_iter().map(|(tx_id, _)| (*tx_id).into()).collect());

            // Make sure that there is no stray cache entry for the original timestamp.
            let last_possible_entry = tx_status_manager.now() - ttl.as_secs();
            let data = tx_status_manager.inner_data();
            assert!(data.timestamps.keys().all(|&timestamp| timestamp >= last_possible_entry));
        }
    }
}
