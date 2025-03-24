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
    services::transaction_status::TransactionStatus,
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
    fn assert_consistency(&self) {
        use std::collections::HashSet;
        // Each prunable status should be in the pruning queue
        for (status_tx_id, (status_time, _)) in &self.prunable_statuses {
            assert!(self
                .pruning_queue
                .iter()
                .any(|(time, tx_id)| tx_id == status_tx_id && time == status_time));
        }

        // There should be a prunable status with given id for each pruning queue entry
        for (_, tx_id) in self.pruning_queue.iter() {
            assert!(self.prunable_statuses.contains_key(tx_id));
        }

        // Pruning queue should have the same number of unique tx_ids as the number
        // of statuses in the prunable_statuses collection
        let tx_count = self.prunable_statuses.len();
        let tx_count_from_queue = self
            .pruning_queue
            .iter()
            .map(|(_, tx_id)| *tx_id)
            .collect::<HashSet<_>>();
        assert_eq!(tx_count, tx_count_from_queue.len());
    }
}

pub(super) struct TxStatusManager {
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

    pub(crate) fn is_prunable(status: &TransactionStatus) -> bool {
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
    use test_case::test_case;

    use fuel_core_types::{
        services::transaction_status::TransactionStatus,
        tai64::Tai64,
    };

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
}
