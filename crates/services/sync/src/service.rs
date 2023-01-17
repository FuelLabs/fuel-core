//! Service utilities for running fuel sync.
use std::sync::Arc;

use crate::{
    import::Params,
    ports,
    state::State,
};

use super::{
    import,
    sync,
};
use fuel_core_services::{
    KillSwitch,
    RunnableService,
    RunnableTask,
    ServiceRunner,
    SharedMutex,
    StateWatcher,
};
use fuel_core_types::blockchain::primitives::BlockHeight;
use futures::{
    stream::BoxStream,
    FutureExt,
};
use tokio::{
    sync::Notify,
    task::JoinHandle,
};

#[cfg(test)]
mod tests;

/// Task that sets up the sync service to run.
pub struct TaskSetup<P, E, C>
where
    P: ports::PeerToPeerPort + Send + Sync + 'static,
    E: ports::ExecutorPort + Send + Sync + 'static,
    C: ports::ConsensusPort + Send + Sync + 'static,
{
    height_stream: BoxStream<'static, BlockHeight>,
    state: SharedMutex<State>,
    params: Params,
    ports: ports::Ports<P, E, C>,
}

/// The task that waits for the service to continue running.
pub struct Task {
    sync_task: Option<JoinHandle<()>>,
    import_task: Option<JoinHandle<anyhow::Result<bool>>>,
    kill_switch: KillSwitch,
}

impl<P, E, C> TaskSetup<P, E, C>
where
    P: ports::PeerToPeerPort + Send + Sync + 'static,
    E: ports::ExecutorPort + Send + Sync + 'static,
    C: ports::ConsensusPort + Send + Sync + 'static,
{
    /// Create a new task setup.
    pub fn new(
        height_stream: BoxStream<'static, BlockHeight>,
        state: State,
        params: Params,
        ports: ports::Ports<P, E, C>,
    ) -> Self {
        let state = SharedMutex::new(state);
        Self {
            height_stream,
            state,
            params,
            ports,
        }
    }
}

#[async_trait::async_trait]
impl RunnableTask for Task {
    async fn run(
        &mut self,
        watcher: &mut fuel_core_services::StateWatcher,
    ) -> anyhow::Result<bool> {
        use futures::future::{
            select,
            Either,
        };

        // Check we still have tasks to run otherwise return false.
        match (&mut self.sync_task, &mut self.import_task) {
            (Some(sync_task), Some(import_task)) => {
                // Select over the task runner state, the sync task and the import task.
                // Match on either the sync task or the import task completing.
                // If the task runner state changes then we check it below.
                if let Either::Right((join_handles, _)) =
                    select(watcher.changed().boxed(), select(sync_task, import_task))
                        .await
                {
                    match join_handles {
                        // Sync task has completed.
                        Either::Left((sync_result, import_task)) => match sync_result {
                            // Sync task has panicked.
                            Err(e) if e.is_panic() => {
                                // Kill the import task and resume the panic.
                                self.kill_switch.kill_all();
                                let _ = import_task.await;
                                std::panic::resume_unwind(e.into_panic());
                            }
                            _ => unreachable!(
                                "The sync task has no errors so this can't happen"
                            ),
                        },
                        // Import task has completed.
                        Either::Right((import_result, sync_task)) => {
                            match import_result {
                                Ok(task_result) => return task_result,
                                Err(e) => {
                                    // If the import task has panicked then we kill the sync task
                                    // and resume the panic.
                                    if e.is_panic() {
                                        self.kill_switch.kill_all();
                                        let _ = sync_task.await;
                                        std::panic::resume_unwind(e.into_panic());
                                    } else {
                                        return Err(e.into())
                                    }
                                }
                            }
                        }
                    }
                }
            }
            _ => return Ok(false),
        }

        // Check if the state of the task runner is still running.
        if !watcher.borrow().started() {
            // The task runner has stopped so we kill the sync and import tasks.
            self.kill_switch.kill_all();
            if let Some(sync_task) = self.sync_task.take() {
                let _ = sync_task.await;
            }
            if let Some(import_task) = self.import_task.take() {
                let _ = import_task.await;
            }
            Ok(false)
        } else {
            Ok(true)
        }
    }
}

#[async_trait::async_trait]
impl<P, E, C> RunnableService for TaskSetup<P, E, C>
where
    P: ports::PeerToPeerPort + Send + Sync + 'static,
    E: ports::ExecutorPort + Send + Sync + 'static,
    C: ports::ConsensusPort + Send + Sync + 'static,
{
    const NAME: &'static str = "fuel-core-sync";

    type SharedData = ();

    type Task = Task;

    fn shared_data(&self) -> Self::SharedData {}

    async fn into_task(self, _: &StateWatcher) -> anyhow::Result<Self::Task> {
        let TaskSetup {
            height_stream,
            state,
            params,
            ports,
        } = self;
        let kill_switch = KillSwitch::new();
        let notify = Arc::new(Notify::new());

        // Spawn the sync task.
        let sync_task = sync::spawn_sync(
            height_stream,
            state.clone(),
            notify.clone(),
            kill_switch.handle(),
        );

        // Spawn the import task.
        let import_task = tokio::spawn(import::import(
            state,
            notify,
            params,
            ports,
            kill_switch.handle(),
            #[cfg(test)]
            || (),
        ));

        Ok(Task {
            sync_task: Some(sync_task),
            import_task: Some(import_task),
            kill_switch,
        })
    }
}

/// Creates an instance of runnable sync service.
pub fn new_service<P, E, C>(
    current_fuel_block_height: BlockHeight,
    p2p: P,
    executor: E,
    consensus: C,
    params: Params,
) -> anyhow::Result<ServiceRunner<TaskSetup<P, E, C>>>
where
    P: ports::PeerToPeerPort + Send + Sync + 'static,
    E: ports::ExecutorPort + Send + Sync + 'static,
    C: ports::ConsensusPort + Send + Sync + 'static,
{
    let height_stream = p2p.height_stream();
    let state = State::new(*current_fuel_block_height, None);
    let ports = ports::Ports {
        p2p: Arc::new(p2p),
        executor: Arc::new(executor),
        consensus: Arc::new(consensus),
    };

    Ok(ServiceRunner::new(TaskSetup::new(
        height_stream,
        state,
        params,
        ports,
    )))
}
