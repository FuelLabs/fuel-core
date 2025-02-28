use fuel_core_services::{
    RunnableService,
    RunnableTask,
    ServiceRunner,
    StateWatcher,
    TaskNextAction,
};
use fuel_core_types::services::p2p::TransactionStatusGossipData;

use crate::{
    ports::P2PSubscriptions,
    SharedState,
};

pub struct Task {
    shared_state: SharedState,
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
                return TaskNextAction::Stop
            }
        }

        TaskNextAction::Continue
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

    ServiceRunner::new(Task {
        shared_state: SharedState::new(),
    })
}
