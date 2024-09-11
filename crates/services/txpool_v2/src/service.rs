use fuel_core_services::{
    RunnableService,
    RunnableTask,
    ServiceRunner,
    StateWatcher,
};
use fuel_core_types::fuel_tx::Transaction;

#[derive(Clone)]
pub struct SharedState;

impl SharedState {
    // TODO: Implement conversion from `Transaction` to `PoolTransaction`. (with all the verifications that it implies): https://github.com/FuelLabs/fuel-core/issues/2186
    fn insert(&mut self, transactions: Vec<Transaction>) -> Vec<()> {
        // Move verif of wasm there
        vec![]
    }
}

pub type Service = ServiceRunner<Task>;

pub struct Task {
    shared_state: SharedState,
}

#[async_trait::async_trait]
impl RunnableService for Task {
    const NAME: &'static str = "TxPoolv2";

    type SharedData = SharedState;

    type Task = Task;

    type TaskParams = ();

    fn shared_data(&self) -> Self::SharedData {
        self.shared_state.clone()
    }

    async fn into_task(
        mut self,
        _: &StateWatcher,
        _: Self::TaskParams,
    ) -> anyhow::Result<Self::Task> {
        Ok(self)
    }
}

#[async_trait::async_trait]
impl RunnableTask for Task {
    async fn run(&mut self, watcher: &mut StateWatcher) -> anyhow::Result<bool> {
        let should_continue;
        tokio::select! {
            _ = watcher.while_started() => {
                should_continue = false;
            }
        }
        Ok(should_continue)
    }

    async fn shutdown(self) -> anyhow::Result<()> {
        Ok(())
    }
}

pub fn new_service() -> Service {
    Service::new(Task {
        shared_state: SharedState,
    })
}
