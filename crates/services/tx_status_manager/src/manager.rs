use anyhow::anyhow;
use std::{
    collections::HashMap,
    sync::{
        Arc,
        Mutex,
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

#[derive(Clone)]
pub struct TxStatusManager {
    statuses: Arc<Mutex<HashMap<TxId, TransactionStatus>>>,
    tx_status_change: TxStatusChange,
}

impl TxStatusManager {
    pub fn new(tx_status_change: TxStatusChange) -> Self {
        Self {
            statuses: Arc::new(Mutex::new(HashMap::new())),
            tx_status_change,
        }
    }

    pub fn status_update(&self, tx_id: TxId, tx_status: TransactionStatus) {
        tracing::debug!(%tx_id, ?tx_status, "new tx status");

        // TODO[RC]: Capacity checks? - Protected by TxPool capacity checks, except for the squeezed state. Maybe introduce some limit.
        // TODO[RC]: Purge old statuses? - Remove the status from the manager upon putting the status into storage.
        // TODO[RC]: Shall we store squeezed out variants as well?
        self.statuses
            .lock()
            .expect("mutex poisoned")
            .insert(tx_id, tx_status.clone());

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
        self.statuses
            .lock()
            .expect("mutex poisoned")
            .get(tx_id)
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
