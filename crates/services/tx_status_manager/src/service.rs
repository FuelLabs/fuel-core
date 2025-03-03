use fuel_core_services::{
    RunnableService,
    RunnableTask,
    ServiceRunner,
    StateWatcher,
    TaskNextAction,
};
use fuel_core_types::{
    fuel_tx::TxId,
    services::{
        p2p::{
            GossipData,
            Preconfirmation,
            PreconfirmationMessage,
            Preconfirmations,
            PreconfirmationsGossipData,
            Sealed,
        },
        txpool::TransactionStatus,
    },
    tai64::Tai64,
};
use futures::StreamExt;

use crate::{
    manager::TxStatusManager,
    ports::P2PSubscriptions,
    subscriptions::Subscriptions,
};

pub struct Task {
    manager: TxStatusManager,
    subscriptions: Subscriptions,
}

impl Task {
    fn new_preconfirmations_from_p2p(
        &mut self,
        preconfirmations: Sealed<Preconfirmations>,
    ) {
        let Sealed {
            signature,
            entity: preconfirmations,
        } = preconfirmations;

        // TODO[RC]: Add test for timestamp verification
        let current_time = Tai64::now();
        if current_time > preconfirmations.expiration() {
            return;
        }

        preconfirmations
            .iter()
            .for_each(|Preconfirmation { tx_id, status }| {
                self.manager.upsert_status(&tx_id, status.clone());
            });
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
                if let Some(GossipData { data, message_id, peer_id }) = tx_status_from_p2p {
                    if let Some(tx_status) = data {
                        match tx_status {
                            PreconfirmationMessage::Preconfirmations(sealed)=>
                            {
                                self.new_preconfirmations_from_p2p(sealed);
                            },
                            PreconfirmationMessage::Delegate(_) => {
                                // We're not interested in delegate messages
                            },}
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

pub fn new_service<P2P>(p2p: P2P) -> ServiceRunner<Task>
where
    P2P: P2PSubscriptions<GossipedStatuses = PreconfirmationsGossipData>,
{
    let tx_status_from_p2p_stream = p2p.gossiped_tx_statuses();

    let subscriptions = Subscriptions {
        new_tx_status: tx_status_from_p2p_stream,
    };

    ServiceRunner::new(Task {
        subscriptions,
        manager: TxStatusManager::new(),
    })
}
