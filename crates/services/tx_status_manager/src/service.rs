use fuel_core_services::{
    RunnableService,
    RunnableTask,
    ServiceRunner,
    StateWatcher,
    TaskNextAction,
};
use fuel_core_types::services::{
    p2p::{
        GossipData,
        PreConfirmationMessage,
        Sealed,
    },
    preconfirmation::Preconfirmation,
    txpool::TransactionStatus,
};
use futures::StreamExt;

use crate::{
    config::Config,
    manager::{
        TimeProvider,
        TxStatusManager,
    },
    ports::{
        P2PPreConfirmationGossipData,
        P2PPreConfirmationMessage,
        P2PSubscriptions,
    },
    subscriptions::Subscriptions,
    update_sender::TxStatusChange,
};

#[derive(Clone, Default)]
pub struct SystemTimeProvider;
impl SystemTimeProvider {
    pub fn new() -> Self {
        Self
    }
}
impl TimeProvider for SystemTimeProvider {}

pub struct Task<Time>
where
    Time: TimeProvider,
{
    manager: TxStatusManager<Time>,
    subscriptions: Subscriptions,
}

impl<Time> Task<Time>
where
    Time: TimeProvider,
{
    fn new_preconfirmations_from_p2p(
        &mut self,
        preconfirmations: P2PPreConfirmationMessage,
    ) {
        match preconfirmations {
            PreConfirmationMessage::Delegate(_sealed) => {
                // TODO[RC]: Handle the delegate message,
            }
            PreConfirmationMessage::Preconfirmations(sealed) => {
                let Sealed {
                    signature: _,
                    entity,
                } = sealed;
                // TODO[RC]: Add signature verification
                entity.preconfirmations.into_iter().for_each(
                    |Preconfirmation { tx_id, status }| {
                        let status: TransactionStatus = status.into();
                        self.manager.status_update(tx_id, status);
                    },
                );
            }
        }
    }
}

#[async_trait::async_trait]
impl<Time> RunnableService for Task<Time>
where
    Time: TimeProvider + Send + Sync + Clone + 'static,
{
    const NAME: &'static str = "TxStatusManagerTask";
    type SharedData = TxStatusManager<Time>;
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

impl<Time> RunnableTask for Task<Time>
where
    Time: TimeProvider + Send + Sync + Clone + 'static,
{
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

pub fn new_service<P2P>(
    p2p: P2P,
    config: Config,
) -> ServiceRunner<Task<SystemTimeProvider>>
where
    P2P: P2PSubscriptions<GossipedStatuses = P2PPreConfirmationGossipData>,
    SystemTimeProvider: Clone,
{
    let tx_status_from_p2p_stream = p2p.gossiped_tx_statuses();
    let subscriptions = Subscriptions {
        new_tx_status: tx_status_from_p2p_stream,
    };

    let system_time_provider = SystemTimeProvider;

    let tx_status_sender = TxStatusChange::new(
        config.max_tx_update_subscriptions,
        // The connection should be closed automatically after the `SqueezedOut` event.
        // But because of slow/malicious consumers, the subscriber can still be occupied.
        // We allow the subscriber to receive the event produced by TxPool's TTL.
        // But we still want to drop subscribers after `2 * TxPool_TTL`.
        config.max_txs_ttl.saturating_mul(2),
    );
    let tx_status_manager = TxStatusManager::new(
        tx_status_sender,
        config.max_tx_status_ttl,
        system_time_provider,
    );

    ServiceRunner::new(Task {
        subscriptions,
        manager: tx_status_manager,
    })
}
