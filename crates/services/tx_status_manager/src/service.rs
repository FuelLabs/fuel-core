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
    config::Config,
    manager::TxStatusManager,
    ports::P2PSubscriptions,
    subscriptions::Subscriptions,
    update_sender::TxStatusChange,
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

        // let expiration = preconfirmations.expiration();
        // let public_key = get_pub_key_by_expiration(expiration);

        // if signature_verified() {
        preconfirmations
            .iter()
            .for_each(|Preconfirmation { tx_id, status }| {
                self.manager.upsert_status(&tx_id, status.clone());
            });
        //} else {
        // TODO[RC]: Log the error
        //}
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
                    if let Some(tx_status) = data {
                        match tx_status {
                            PreconfirmationMessage::Preconfirmations(sealed)=>
                            {
                                self.new_preconfirmations_from_p2p(sealed);
                            },
                            PreconfirmationMessage::Delegate(sealed) => {
                                // TODO[RC]: Properly handle the signature verification
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
