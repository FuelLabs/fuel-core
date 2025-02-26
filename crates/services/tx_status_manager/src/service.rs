use fuel_core_services::{
    RunnableService,
    RunnableTask,
    ServiceRunner,
    StateWatcher,
    TaskNextAction,
};

#[derive(Clone)]
pub struct SharedState {}

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
        todo!()
    }
}

impl RunnableTask for Task {
    async fn run(
        &mut self,
        _watcher: &mut fuel_core_services::StateWatcher,
    ) -> TaskNextAction {
        todo!()
    }

    async fn shutdown(self) -> anyhow::Result<()> {
        Ok(())
    }
}

pub fn new_service() -> ServiceRunner<Task> {
    todo!()
}
