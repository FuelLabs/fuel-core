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
            TransactionStatusGossipData,
        },
        txpool::TransactionStatus,
    },
};
use futures::StreamExt;

use crate::{
    ports::P2PSubscriptions,
    subscriptions::Subscriptions,
    SharedState,
};

pub struct Task {
    shared_state: SharedState,
    subscriptions: Subscriptions,
}

impl Task {
    fn new_tx_status_from_p2p(&mut self, (tx_id, tx_status): (TxId, TransactionStatus)) {
        // TODO[RC]: Capacity checks?
        // TODO[RC]: Purge old statuses?
        self.shared_state.add_status(&tx_id, tx_status);
    }
}

#[async_trait::async_trait]
impl RunnableService for Task {
    const NAME: &'static str = "TxStatusManagerTask";
    type SharedData = SharedState;
    type Task = Self;
    type TaskParams = ();

    fn shared_data(&self) -> Self::SharedData {
        self.shared_state.clone()
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
                        self.new_tx_status_from_p2p(tx_status);
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
    P2P: P2PSubscriptions<GossipedStatuses = TransactionStatusGossipData>,
{
    let tx_status_from_p2p_stream = p2p.gossiped_tx_statuses();

    let subscriptions = Subscriptions {
        new_tx_status: tx_status_from_p2p_stream,
    };

    ServiceRunner::new(Task {
        shared_state: SharedState::new(),
        subscriptions,
    })
}
