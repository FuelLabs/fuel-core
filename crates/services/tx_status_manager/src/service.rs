use fuel_core_services::{
    RunnableService,
    RunnableTask,
    ServiceRunner,
    StateWatcher,
    TaskNextAction,
};
use fuel_core_types::services::p2p::{
    GossipData,
    Preconfirmation,
    PreconfirmationMessage,
    PreconfirmationsGossipData,
    Sealed,
};
use futures::StreamExt;

use crate::{
    config::Config,
    manager::TxStatusManager,
    ports::P2PSubscriptions,
    subscriptions::Subscriptions,
    tx_status_stream::TxUpdate,
    update_sender::TxStatusChange,
};

pub struct Task {
    manager: TxStatusManager,
    subscriptions: Subscriptions,
}

impl Task {
    fn new_preconfirmations_from_p2p(
        &mut self,
        preconfirmations: PreconfirmationMessage,
    ) {
        match preconfirmations {
            PreconfirmationMessage::Delegate(_sealed) => {
                // TODO[RC]: Handle the delegate message,
            }
            PreconfirmationMessage::Preconfirmations(sealed) => {
                let Sealed {
                    signature: _,
                    entity: preconfirmations,
                } = sealed;
                // TODO[RC]: Add signature verification
                preconfirmations
                    .iter()
                    .for_each(|Preconfirmation { tx_id, status }| {
                        self.manager
                            .status_update(TxUpdate::new(*tx_id, status.into()));
                    });
            }
        }
    }
}

#[async_trait::async_trait]
impl RunnableService for Task {
    const NAME: &'static str = "TxStatusManagerTask";
    type SharedData = TxStatusManager;
    type Task = Self;
    type TaskParams = ();

    fn shared_data(&self) -> Self::SharedData {
        self.manager.clone()
    }

    async fn into_task(
        self,
        _: &StateWatcher,
        _: Self::TaskParams,
    ) -> anyhow::Result<Self::Task> {
        Ok(self)
    }
}

impl RunnableTask for Task {
    async fn run(
        &mut self,
        watcher: &mut fuel_core_services::StateWatcher,
    ) -> TaskNextAction {
        tokio::select! {
            biased;

            _ = watcher.while_started() => {
                TaskNextAction::Stop
            }

            tx_status_from_p2p = self.subscriptions.new_tx_status.next() => {
                if let Some(GossipData { data, .. }) = tx_status_from_p2p {
                    if let Some(msg) = data {
                        self.new_preconfirmations_from_p2p(msg);
                    }
                    TaskNextAction::Continue
                } else {
                    TaskNextAction::Stop
                }
            }

        }
    }

    async fn shutdown(self) -> anyhow::Result<()> {
        Ok(())
    }
}

pub fn new_service<P2P>(p2p: P2P, config: Config) -> ServiceRunner<Task>
where
    P2P: P2PSubscriptions<GossipedStatuses = PreconfirmationsGossipData>,
{
    let tx_status_from_p2p_stream = p2p.gossiped_tx_statuses();
    let subscriptions = Subscriptions {
        new_tx_status: tx_status_from_p2p_stream,
    };

    let tx_status_sender = TxStatusChange::new(
        config.max_tx_update_subscriptions,
        // The connection should be closed automatically after the `SqueezedOut` event.
        // But because of slow/malicious consumers, the subscriber can still be occupied.
        // We allow the subscriber to receive the event produced by TxPool's TTL.
        // But we still want to drop subscribers after `2 * TxPool_TTL`.
        config.max_txs_ttl.saturating_mul(2),
    );
    let tx_status_manager = TxStatusManager::new(tx_status_sender);

    ServiceRunner::new(Task {
        subscriptions,
        manager: tx_status_manager,
    })
}
