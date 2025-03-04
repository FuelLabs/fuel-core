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
use tokio::sync::broadcast;

use crate::{
    tx_status_stream::{
        TxStatusMessage,
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

    pub fn status_update(&self, tx_update: TxUpdate) {
        tracing::debug!(?tx_update, "new tx update");

        match tx_update.message {
            TxStatusMessage::Status(ref tx_status) => {
                match tx_status {
                    TransactionStatus::Submitted { .. } => {
                        if let Err(err) = self
                            .tx_status_change
                            .new_tx_notification_sender
                            .send(tx_update.tx_id)
                        {
                            tracing::error!(%err, "new_tx_notification_sender failed");
                        }
                    }
                    TransactionStatus::Success { .. }
                    | TransactionStatus::SqueezedOut { .. }
                    | TransactionStatus::Failed { .. } => (),
                };

                // TODO[RC]: Capacity checks? - Protected by TxPool capacity checks, except for the squeezed state. Maybe introduce some limit.
                // TODO[RC]: Purge old statuses? - Remove the status from the manager upon putting the status into storage.
                // TODO[RC]: Shall we store squeezed out variants as well?
                self.statuses
                    .lock()
                    .expect("mutex poisoned")
                    .insert(tx_update.tx_id, tx_status.clone());
            }
            TxStatusMessage::FailedStatus => todo!(),
        }

        self.tx_status_change.update_sender.send(&tx_update);
    }

    pub fn status(&self, tx_id: &TxId) -> Option<TransactionStatus> {
        self.statuses
            .lock()
            .expect("mutex poisoned")
            .get(tx_id)
            .map(|s| s.clone())
    }

    /// Subscribe to new transaction notifications.
    pub fn new_tx_notification_subscribe(&self) -> broadcast::Receiver<TxId> {
        self.tx_status_change.new_tx_notification_sender.subscribe()
    }

    /// Subscribe to status updates for a transaction.
    pub fn tx_update_subscribe(&self, tx_id: Bytes32) -> anyhow::Result<TxStatusStream> {
        self.tx_status_change
            .update_sender
            .try_subscribe::<MpscChannel>(tx_id)
            .ok_or(anyhow!("Maximum number of subscriptions reached"))
    }
}
