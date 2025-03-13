use anyhow::anyhow;
use std::{
    collections::{
        hash_map::Entry,
        HashMap,
        VecDeque,
    },
    time::Duration,
};
use tokio::time::Instant;

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

use fuel_core_metrics::tx_status_manager_metrics::metrics_manager;

pub struct Data {
    pruning_queue: VecDeque<(Instant, TxId)>,
    non_prunable_statuses: HashMap<TxId, TransactionStatus>,
    prunable_statuses: HashMap<TxId, (Instant, TransactionStatus)>,
}

impl Data {
    pub fn empty() -> Self {
        Self {
            pruning_queue: VecDeque::new(),
            prunable_statuses: HashMap::new(),
            non_prunable_statuses: HashMap::new(),
        }
    }

    #[cfg(test)]
    fn all_but_prunable_statuses(
        &self,
    ) -> impl Iterator<Item = (&Bytes32, &(Instant, TransactionStatus))> {
        self.prunable_statuses
            .iter()
            .filter(|(_, (_, status))| TxStatusManager::is_prunable(status))
    }

    #[cfg(test)]
    fn assert_consistency(&self) {
        use std::collections::HashSet;
        // There should be a timestamp entry for each registered tx id
        for (status_tx_id, (status_time, _)) in self.all_but_prunable_statuses() {
            assert!(self
                .pruning_queue
                .iter()
                .any(|(time, tx_id)| tx_id == status_tx_id && time == status_time));
        }

        // There should be a transaction with given id for each timestamp cache
        for (_, tx_id) in self.pruning_queue.iter() {
            assert!(self.prunable_statuses.contains_key(tx_id));
        }

        // The count of transactions in both collections must match
        let tx_count = self.all_but_prunable_statuses().count();
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
    metrics: bool,
}

impl TxStatusManager {
    pub fn new(tx_status_change: TxStatusChange, ttl: Duration, metrics: bool) -> Self {
        Self {
            data: Data::empty(),
            tx_status_change,
            ttl,
            metrics,
        }
    }

    fn is_prunable(status: &TransactionStatus) -> bool {
        !matches!(status, TransactionStatus::Submitted(_))
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

            let entry = self.data.prunable_statuses.entry(tx_id);
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
                    let (timestamp, _) = entry.get();
                    if *timestamp == past {
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
        if TxStatusManager::is_prunable(&tx_status) {
            self.data.pruning_queue.push_front((now, tx_id));
            self.data.prunable_statuses.insert(tx_id, (now, tx_status));
            self.data.non_prunable_statuses.remove(&tx_id);
        } else {
            self.data.non_prunable_statuses.insert(tx_id, tx_status);
        }
    }

    fn register_status(&mut self, tx_id: TxId, tx_status: TransactionStatus) {
        self.prune_old_statuses();
        self.add_new_status(tx_id, tx_status);
    }

    pub fn status_update(&mut self, tx_id: TxId, tx_status: TransactionStatus) {
        tracing::debug!(%tx_id, ?tx_status, "new tx status");

        self.register_status(tx_id, tx_status.clone());

        self.tx_status_change
            .update_sender
            .send(TxUpdate::new(tx_id, tx_status.into()));

        if self.metrics {
            metrics_manager()
                .prunable_status_count
                .set(self.data.prunable_statuses.len() as i64);
            metrics_manager()
                .non_prunable_status_count
                .set(self.data.non_prunable_statuses.len() as i64);
            metrics_manager()
                .pruning_queue_len
                .set(self.data.pruning_queue.len() as i64);
            metrics_manager().pruning_queue_oldest_status_age_s.set(
                self.data
                    .pruning_queue
                    .back()
                    .map(|(time, _)| time.elapsed().as_secs() as i64)
                    .unwrap_or(0),
            );
        };
    }

    pub fn status(&self, tx_id: &TxId) -> Option<&TransactionStatus> {
        self.data.non_prunable_statuses.get(tx_id).or_else(|| {
            self.data
                .prunable_statuses
                .get(tx_id)
                .map(|(_, status)| status)
        })
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
    use std::{
        collections::HashSet,
        time::Duration,
    };

    use test_case::test_case;

    use fuel_core_types::{
        fuel_crypto::rand::{
            rngs::StdRng,
            seq::SliceRandom,
            SeedableRng,
        },
        fuel_tx::Bytes32,
        services::txpool::TransactionStatus,
        tai64::Tai64,
    };

    use crate::update_sender::TxStatusChange;

    use super::TxStatusManager;

    fn submitted() -> TransactionStatus {
        TransactionStatus::submitted(Tai64::UNIX_EPOCH)
    }

    fn success() -> TransactionStatus {
        TransactionStatus::Success(Default::default())
    }

    fn preconfirmation_success() -> TransactionStatus {
        TransactionStatus::PreConfirmationSuccess(Default::default())
    }

    fn squeezed_out() -> TransactionStatus {
        TransactionStatus::squeezed_out("fishy tx".to_string())
    }

    fn preconfirmation_squeezed_out() -> TransactionStatus {
        TransactionStatus::preconfirmation_squeezed_out(
            "fishy preconfirmation".to_string(),
        )
    }

    fn failure() -> TransactionStatus {
        TransactionStatus::Failure(Default::default())
    }

    fn preconfirmation_failure() -> TransactionStatus {
        TransactionStatus::PreConfirmationFailure(Default::default())
    }

    fn all_statuses() -> [TransactionStatus; 7] {
        [
            submitted(),
            success(),
            preconfirmation_success(),
            squeezed_out(),
            preconfirmation_squeezed_out(),
            failure(),
            preconfirmation_failure(),
        ]
    }

    fn random_prunable_tx_status(rng: &mut StdRng) -> TransactionStatus {
        all_statuses()
            .into_iter()
            .filter(TxStatusManager::is_prunable)
            .collect::<Vec<_>>()
            .choose(rng)
            .unwrap()
            .clone()
    }

    fn random_tx_status(rng: &mut StdRng) -> TransactionStatus {
        all_statuses().choose(rng).unwrap().clone()
    }

    const TTL: Duration = Duration::from_secs(4);
    const QUART_OF_TTL: Duration = Duration::from_secs(1);
    const HALF_OF_TTL: Duration = Duration::from_secs(2);

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
                tx_status_manager.status(&tx_id) == Some(&status),
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

    #[test_case(submitted() => false)]
    #[test_case(success() => true)]
    #[test_case(preconfirmation_success() => true)]
    #[test_case(squeezed_out() => true)]
    #[test_case(preconfirmation_squeezed_out() => true)]
    #[test_case(failure() => true)]
    #[test_case(preconfirmation_failure() => true)]
    fn is_prunable(status: TransactionStatus) -> bool {
        TxStatusManager::is_prunable(&status)
    }

    mod equal_ids {
        use std::time::Duration;

        use crate::{
            manager::tests::{
                failure,
                preconfirmation_failure,
                preconfirmation_squeezed_out,
                preconfirmation_success,
                squeezed_out,
                submitted,
                success,
            },
            update_sender::TxStatusChange,
            TxStatusManager,
        };

        use super::{
            assert_absence,
            assert_presence,
            HALF_OF_TTL,
            QUART_OF_TTL,
            TTL,
        };

        #[tokio::test(start_paused = true)]
        async fn simple_registration() {
            let tx_status_change = TxStatusChange::new(100, Duration::from_secs(360));
            let mut tx_status_manager =
                TxStatusManager::new(tx_status_change, TTL, false);

            let tx1_id = [1u8; 32].into();
            let tx2_id = [2u8; 32].into();
            let tx3_id = [3u8; 32].into();
            let tx4_id = [4u8; 32].into();
            let tx5_id = [5u8; 32].into();
            let tx6_id = [6u8; 32].into();
            let tx7_id = [7u8; 32].into();
            let tx99_id = [99u8; 32].into();

            // Register tx1 and tx2
            tx_status_manager.status_update(tx1_id, submitted());
            tx_status_manager.status_update(tx2_id, success());
            tx_status_manager.status_update(tx3_id, preconfirmation_success());

            // Sleep for less than a TTL
            tokio::time::advance(Duration::from_secs(1)).await;

            tx_status_manager.status_update(tx4_id, squeezed_out());
            tx_status_manager.status_update(tx5_id, preconfirmation_squeezed_out());
            tx_status_manager.status_update(tx6_id, failure());
            tx_status_manager.status_update(tx7_id, preconfirmation_failure());

            // Sleep for less than a TTL
            tokio::time::advance(Duration::from_secs(1)).await;

            // Trigger pruning
            tx_status_manager.status_update(tx99_id, success());

            // All should be present
            assert_presence(
                &tx_status_manager,
                vec![
                    tx1_id, tx2_id, tx3_id, tx4_id, tx5_id, tx6_id, tx7_id, tx99_id,
                ],
            );
        }

        #[tokio::test(start_paused = true)]
        async fn prunes_old_statuses() {
            let tx_status_change = TxStatusChange::new(100, Duration::from_secs(360));
            let mut tx_status_manager =
                TxStatusManager::new(tx_status_change, TTL, false);

            let tx1_id = [1u8; 32].into();
            let tx2_id = [2u8; 32].into();
            let tx3_id = [3u8; 32].into();
            let tx4_id = [4u8; 32].into();

            // Register tx1
            tx_status_manager.status_update(tx1_id, success());
            assert_presence(&tx_status_manager, vec![tx1_id]);

            // Move 2 second forward (half of TTL) and register tx2
            tokio::time::advance(HALF_OF_TTL).await;
            tx_status_manager.status_update(tx2_id, success());

            // Both should be present, since TTL didn't pass yet
            assert_presence(&tx_status_manager, vec![tx1_id, tx2_id]);

            // Move 3 second forward, for a total of 5s.
            // TTL = 4s, so tx1 should be pruned.
            tokio::time::advance(HALF_OF_TTL + QUART_OF_TTL).await;

            // Trigger the pruning
            tx_status_manager.status_update(tx3_id, success());

            // tx1 should be pruned, tx2 and tx3 should be present
            assert_absence(&tx_status_manager, vec![tx1_id]);
            assert_presence(&tx_status_manager, vec![tx2_id, tx3_id]);

            // Move 2 second forward, for a total of 7s.
            // TTL = 4s, so tx2 should be pruned.
            tokio::time::advance(HALF_OF_TTL).await;

            // Trigger the pruning
            tx_status_manager.status_update(tx4_id, success());

            // tx1 and tx2 should be pruned, tx3 and tx4 should be present
            assert_absence(&tx_status_manager, vec![tx1_id, tx2_id]);
            assert_presence(&tx_status_manager, vec![tx3_id, tx4_id]);
        }

        #[tokio::test(start_paused = true)]
        async fn prunes_multiple_old_statuses() {
            let tx_status_change = TxStatusChange::new(100, Duration::from_secs(360));
            let mut tx_status_manager =
                TxStatusManager::new(tx_status_change, TTL, false);

            let tx1_id = [1u8; 32].into();
            let tx2_id = [2u8; 32].into();
            let tx3_id = [3u8; 32].into();
            let tx4_id = [4u8; 32].into();
            let tx5_id = [5u8; 32].into();
            let tx6_id = [6u8; 32].into();

            // Register some transactions
            tx_status_manager.status_update(tx1_id, success());
            tx_status_manager.status_update(tx2_id, success());
            tx_status_manager.status_update(tx3_id, success());

            // Sleep for less than TTL
            tokio::time::advance(QUART_OF_TTL).await;

            // Register some more transactions
            tx_status_manager.status_update(tx4_id, success());
            tx_status_manager.status_update(tx5_id, success());

            // Move beyond TTL
            tokio::time::advance(TTL).await;

            // Trigger the pruning
            tx_status_manager.status_update(tx6_id, success());

            // All but the last one should be pruned.
            assert_absence(
                &tx_status_manager,
                vec![tx1_id, tx2_id, tx3_id, tx4_id, tx5_id],
            );
            assert_presence(&tx_status_manager, vec![tx6_id]);
        }
    }

    mod distinct_ids {

        use crate::{
            manager::tests::{
                failure,
                preconfirmation_failure,
                preconfirmation_success,
                squeezed_out,
                success,
            },
            update_sender::TxStatusChange,
            TxStatusManager,
        };

        use super::{
            assert_absence,
            assert_presence_with_status,
            Duration,
            HALF_OF_TTL,
            QUART_OF_TTL,
            TTL,
        };

        #[tokio::test(start_paused = true)]
        async fn simple_registration() {
            let tx_status_change = TxStatusChange::new(100, Duration::from_secs(360));
            let mut tx_status_manager =
                TxStatusManager::new(tx_status_change, TTL, false);

            let tx1_id = [1u8; 32].into();
            let tx2_id = [2u8; 32].into();

            // Register tx1 and tx2
            tx_status_manager.status_update(tx1_id, preconfirmation_success());
            tx_status_manager.status_update(tx1_id, success());

            // Sleep for less than a TTL
            tokio::time::advance(QUART_OF_TTL).await;

            // Register tx2
            tx_status_manager.status_update(tx2_id, failure());

            // All should be present
            assert_presence_with_status(
                &tx_status_manager,
                vec![(tx1_id, success()), (tx2_id, failure())],
            );
        }

        #[tokio::test(start_paused = true)]
        async fn prunes_old_statuses() {
            let tx_status_change = TxStatusChange::new(100, Duration::from_secs(360));
            let mut tx_status_manager =
                TxStatusManager::new(tx_status_change, TTL, false);

            let tx1_id = [1u8; 32].into();
            let tx2_id = [2u8; 32].into();
            let tx3_id = [3u8; 32].into();
            let tx4_id = [4u8; 32].into();

            // Register tx1 and tx3
            tx_status_manager.status_update(tx1_id, success());
            tx_status_manager.status_update(tx3_id, success());

            // Move 2 second forward (half of TTL), register tx2
            // and update status of tx1
            tokio::time::advance(HALF_OF_TTL).await;
            tx_status_manager.status_update(tx2_id, preconfirmation_success());
            tx_status_manager.status_update(tx1_id, preconfirmation_failure());

            // All should be present, since TTL didn't pass yet
            assert_presence_with_status(
                &tx_status_manager,
                vec![
                    (tx1_id, preconfirmation_failure()),
                    (tx2_id, preconfirmation_success()),
                    (tx3_id, success()),
                ],
            );

            // Move 3 second forward, for a total of 5s.
            // TTL = 4s, so tx1 should be pruned.
            tokio::time::advance(HALF_OF_TTL + QUART_OF_TTL).await;

            // Trigger the pruning
            tx_status_manager.status_update(tx4_id, failure());

            // Only tx3 should be pruned since it's in the manager
            // since the beginning. tx2 was registered later
            // and the status (and timestamp) of tx1 was
            // also update later.
            assert_presence_with_status(
                &tx_status_manager,
                vec![
                    (tx1_id, preconfirmation_failure()),
                    (tx2_id, preconfirmation_success()),
                ],
            );
        }

        #[tokio::test(start_paused = true)]
        async fn prunes_multiple_old_statuses() {
            let tx_status_change = TxStatusChange::new(100, Duration::from_secs(360));
            let mut tx_status_manager =
                TxStatusManager::new(tx_status_change, TTL, false);

            let tx1_id = [1u8; 32].into();
            let tx2_id = [2u8; 32].into();
            let tx3_id = [3u8; 32].into();
            let tx4_id = [4u8; 32].into();
            let tx5_id = [5u8; 32].into();
            let tx6_id = [6u8; 32].into();

            // Register some transactions
            tx_status_manager.status_update(tx1_id, success());
            tx_status_manager.status_update(tx2_id, success());
            tx_status_manager.status_update(tx3_id, success());

            // Sleep for less than TTL
            tokio::time::advance(QUART_OF_TTL).await;

            // Register some more transactions and update
            // some old statuses
            tx_status_manager.status_update(tx4_id, success());
            tx_status_manager.status_update(tx5_id, success());
            tx_status_manager.status_update(tx1_id, squeezed_out());
            tx_status_manager.status_update(tx2_id, squeezed_out());

            // Move beyond TTL
            tokio::time::advance(TTL).await;

            // Trigger the pruning
            tx_status_manager.status_update(tx6_id, failure());

            // All but the last one should be pruned.
            assert_absence(
                &tx_status_manager,
                vec![tx1_id, tx2_id, tx3_id, tx4_id, tx5_id],
            );
            assert_presence_with_status(&tx_status_manager, vec![(tx6_id, failure())]);
        }

        #[tokio::test(start_paused = true)]
        async fn status_preserved_in_cache_when_first_status_expires() {
            let tx_status_change = TxStatusChange::new(100, Duration::from_secs(360));
            let mut tx_status_manager =
                TxStatusManager::new(tx_status_change, TTL, false);

            let tx1_id = [1u8; 32].into();
            let tx2_id = [2u8; 32].into();

            // Register tx1 and remember it's timestamp
            tx_status_manager.status_update(tx1_id, success());

            // Sleep for 3/4 of TTL
            tokio::time::advance(HALF_OF_TTL + QUART_OF_TTL).await;

            // Given
            // Update tx1 status, the timestamp cache should get updated.
            // Now we should have 2 statuses one with TTL deadline, another one with TTL + 3/4 of TTL
            tx_status_manager.status_update(tx1_id, squeezed_out());

            // When
            // Sleep for half of TTL, it should expire the first status
            tokio::time::advance(HALF_OF_TTL).await;
            // It forces cleanup expired statuses and should remove status_1 for tx1
            tx_status_manager.status_update(tx2_id, success());

            // Then
            // The status 2 should remain in the cache for tx1
            let status = tx_status_manager.status(&tx1_id);
            assert!(status.is_some());
        }
    }

    mod submitted_transaction_pruning {
        use std::time::Duration;

        use crate::{
            manager::tests::{
                assert_absence,
                assert_presence_with_status,
                failure,
                squeezed_out,
                submitted,
                success,
                HALF_OF_TTL,
                TTL,
            },
            update_sender::TxStatusChange,
            TxStatusManager,
        };

        #[tokio::test(start_paused = true)]
        async fn submitted_status_is_not_pruned_with_ttl() {
            let tx_status_change = TxStatusChange::new(100, Duration::from_secs(360));
            let mut tx_status_manager =
                TxStatusManager::new(tx_status_change, TTL, false);

            let tx1_id = [1u8; 32].into();
            let tx2_id = [2u8; 32].into();

            // Given
            tx_status_manager.status_update(tx1_id, submitted());
            tx_status_manager.status_update(tx2_id, squeezed_out());

            // When
            tokio::time::advance(TTL + HALF_OF_TTL).await;
            let tx_final = [99u8; 32].into();
            tx_status_manager.status_update(tx_final, submitted());

            // Then
            assert_presence_with_status(
                &tx_status_manager,
                vec![(tx1_id, submitted()), (tx_final, submitted())],
            );
            assert_absence(&tx_status_manager, vec![tx2_id]);
        }

        #[tokio::test(start_paused = true)]
        async fn update_to_submitted_disables_ttl() {
            let tx_status_change = TxStatusChange::new(100, Duration::from_secs(360));
            let mut tx_status_manager =
                TxStatusManager::new(tx_status_change, TTL, false);

            let tx1_id = [1u8; 32].into();
            let tx2_id = [2u8; 32].into();

            // Given
            tx_status_manager.status_update(tx1_id, squeezed_out());
            tx_status_manager.status_update(tx2_id, squeezed_out());

            tokio::time::advance(HALF_OF_TTL).await;

            tx_status_manager.status_update(tx1_id, submitted());
            tx_status_manager.status_update(tx2_id, success());

            assert_presence_with_status(
                &tx_status_manager,
                vec![(tx1_id, submitted()), (tx2_id, success())],
            );

            // When
            tokio::time::advance(TTL + HALF_OF_TTL).await;

            let tx_final = [99u8; 32].into();
            tx_status_manager.status_update(tx_final, failure());

            // Then
            // tx1 is back in "submitted", so it should survive pruning
            assert_presence_with_status(
                &tx_status_manager,
                vec![(tx1_id, submitted()), (tx_final, failure())],
            );
            assert_absence(&tx_status_manager, vec![tx2_id]);
        }
    }

    use proptest::prelude::*;
    use std::collections::HashMap;
    use tokio::time::Instant;

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

    #[tokio::main(start_paused = true, flavor = "current_thread")]
    #[allow(clippy::arithmetic_side_effects)]
    async fn _test_status_manager_pruning(ttl: Duration, actions: Vec<Action>) {
        let mut rng = StdRng::seed_from_u64(2322u64);

        let tx_status_change = TxStatusChange::new(100, Duration::from_secs(360));
        let mut tx_status_manager = TxStatusManager::new(tx_status_change, ttl, false);
        let tx_id_pool = generate_tx_id_pool();

        // This will be used to track when each txid was updated so that
        // we can do the final assert against the TTL.
        let mut update_times = HashMap::new();
        let mut non_prunable_ids = HashSet::new();

        // Simulate flow of time and transaction updates
        for action in actions {
            match action {
                Action::UpdateStatus { tx_id_index } => {
                    let tx_id = tx_id_pool[tx_id_index];

                    // Make sure we'll never update back to submitted
                    let current_tx_status = tx_status_manager.status(&tx_id.into());
                    let new_tx_status = match current_tx_status {
                        Some(_) => random_prunable_tx_status(&mut rng),
                        None => random_tx_status(&mut rng),
                    };

                    if TxStatusManager::is_prunable(&new_tx_status) {
                        update_times.insert(tx_id, Instant::now());
                        non_prunable_ids.remove(&tx_id);
                    } else {
                        non_prunable_ids.insert(tx_id);
                    }
                    tx_status_manager.status_update(tx_id.into(), new_tx_status);
                }
                Action::AdvanceTime { seconds } => {
                    tokio::time::advance(Duration::from_secs(seconds)).await;
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
        tx_status_manager.status_update(final_tx_id.into(), failure());
        update_times.insert(final_tx_id, Instant::now());

        // Verify that only recent transactions are present
        let (recent_tx_ids, not_recent_tx_ids): (Vec<_>, Vec<_>) = update_times
            .iter()
            .partition(|(_, &time)| time + ttl > Instant::now());
        assert_presence(
            &tx_status_manager,
            recent_tx_ids
                .into_iter()
                .map(|(tx_id, _)| (*tx_id).into())
                .chain(non_prunable_ids.iter().cloned().map(Into::into))
                .collect(),
        );
        assert_absence(
            &tx_status_manager,
            not_recent_tx_ids
                .into_iter()
                .filter(|(tx_id, _)| !non_prunable_ids.contains(*tx_id))
                .map(|(tx_id, _)| (*tx_id).into())
                .collect(),
        );
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        #[allow(clippy::arithmetic_side_effects)]
        fn test_status_manager_pruning(
            ttl in ttl_strategy(MIN_TTL, MAX_TTL),
            actions in actions_strategy(MIN_ACTIONS, MAX_ACTIONS)
        ) {
            _test_status_manager_pruning(ttl, actions);
        }
    }
}
