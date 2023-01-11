use crate::state::{
    State,
    StateWatcher,
};
use anyhow::anyhow;
use tokio::{
    sync::watch,
    task::JoinHandle,
};

/// Alias for Arc<T>
pub type Shared<T> = std::sync::Arc<T>;

/// Used if services have no asynchronously shared data
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmptyShared;

/// Trait for service runners, providing a minimal interface for managing
/// the lifecycle of services such as start/stop and health status.
#[async_trait::async_trait]
pub trait Service {
    /// Send a start signal to the service without waiting for it to start.
    /// Returns an error if the service was already started.
    fn start(&self) -> anyhow::Result<()>;

    /// Send a start signal to the service and wait for it to start up.
    /// Returns an error if the service was already started.
    async fn start_and_await(&self) -> anyhow::Result<State>;

    /// Wait for service to start or stop (without sending any signal).
    async fn await_start_or_stop(&self) -> anyhow::Result<State>;

    /// Send a stop signal to the service without waiting for it to shutdown.
    /// Returns false if the service was already stopped, true if it is running.
    fn stop(&self) -> bool;

    /// Send stop signal to service and wait for it to shutdown.
    async fn stop_and_await(&self) -> anyhow::Result<State>;

    /// Wait for service to stop (without sending a stop signal).
    async fn await_stop(&self) -> anyhow::Result<State>;

    /// The current state of the service (i.e. `Started`, `Stopped`, etc..)
    fn state(&self) -> State;
}

/// Trait used by `ServiceRunner` to encapsulate the business logic tasks for a service.
#[async_trait::async_trait]
pub trait RunnableService: Send {
    /// The name of the runnable service, used for namespacing error messages.
    const NAME: &'static str;

    /// Service specific shared data. This is used when you have data that needs to be shared by
    /// one or more tasks. It is the implementors responsibility to ensure cloning this
    /// type is shallow and doesn't provide a full duplication of data that is meant
    /// to be shared between asynchronous processes.
    type SharedData: Clone + Send + Sync;

    /// The initialized runnable task type.
    type Task: RunnableTask;

    /// A cloned instance of the shared data
    fn shared_data(&self) -> Self::SharedData;

    /// Converts the service into a runnable task before the main run loop.
    ///
    /// The `state` is a `State` watcher of the service. Some tasks may handle state changes
    /// on their own.
    async fn into_task(self, state_watcher: &StateWatcher) -> anyhow::Result<Self::Task>;
}

/// The trait is implemented by the service task and contains a single iteration of the infinity
/// loop.
#[async_trait::async_trait]
pub trait RunnableTask: Send {
    /// This function should contain the main business logic of the service task. It will run until
    /// the service either returns false, panics or a stop signal is received.
    /// If the service returns an error, it will be logged and execution will resume.
    /// This is intended to be called only by the `ServiceRunner`.
    ///
    /// The `ServiceRunner` continue to call the `run` method in the loop while the state is
    /// `State::Started`. So first, the `run` method should return a value, and after, the service
    /// will stop. If the service should react to the state change earlier, it should handle it in
    /// the `run` loop on its own. See [`StateWatcher::while_started`].
    async fn run(&mut self, watcher: &mut StateWatcher) -> anyhow::Result<bool>;
}

/// The service runner manages the lifecycle, execution and error handling of a `RunnableService`.
/// It can be cloned and passed between threads.
#[derive(Debug)]
pub struct ServiceRunner<S>
where
    S: RunnableService,
{
    /// The shared state of the service
    pub shared: S::SharedData,
    state: Shared<watch::Sender<State>>,
}

impl<S> Clone for ServiceRunner<S>
where
    S: RunnableService,
{
    fn clone(&self) -> Self {
        Self {
            shared: self.shared.clone(),
            state: self.state.clone(),
        }
    }
}

impl<S> ServiceRunner<S>
where
    S: RunnableService + 'static,
{
    /// Initializes a new `ServiceRunner` containing a `RunnableService`
    pub fn new(service: S) -> Self {
        let shared = service.shared_data();
        let state = initialize_loop(service);
        Self { shared, state }
    }

    async fn _await_start_or_stop(
        &self,
        mut start: StateWatcher,
    ) -> anyhow::Result<State> {
        loop {
            let state = start.borrow().clone();
            if !state.starting() {
                return Ok(state)
            }
            start.changed().await?;
        }
    }

    async fn _await_stop(&self, mut stop: StateWatcher) -> anyhow::Result<State> {
        loop {
            let state = stop.borrow().clone();
            if state.stopped() {
                return Ok(state)
            }
            stop.changed().await?;
        }
    }
}

#[async_trait::async_trait]
impl<S> Service for ServiceRunner<S>
where
    S: RunnableService + 'static,
{
    fn start(&self) -> anyhow::Result<()> {
        let started = self.state.send_if_modified(|state| {
            if state.not_started() {
                *state = State::Starting;
                true
            } else {
                false
            }
        });

        if started {
            Ok(())
        } else {
            Err(anyhow!(
                "The service `{}` already has been started.",
                S::NAME
            ))
        }
    }

    async fn start_and_await(&self) -> anyhow::Result<State> {
        let start = self.state.subscribe().into();
        self.start()?;
        self._await_start_or_stop(start).await
    }

    async fn await_start_or_stop(&self) -> anyhow::Result<State> {
        let start = self.state.subscribe().into();
        self._await_start_or_stop(start).await
    }

    fn stop(&self) -> bool {
        self.state.send_if_modified(|state| {
            if state.not_started() || state.starting() || state.started() {
                *state = State::Stopping;
                true
            } else {
                false
            }
        })
    }

    async fn stop_and_await(&self) -> anyhow::Result<State> {
        let stop = self.state.subscribe().into();
        self.stop();
        self._await_stop(stop).await
    }

    async fn await_stop(&self) -> anyhow::Result<State> {
        let stop = self.state.subscribe().into();
        self._await_stop(stop).await
    }

    fn state(&self) -> State {
        self.state.borrow().clone()
    }
}

/// Initialize the background loop as a spawned task.
fn initialize_loop<S>(service: S) -> Shared<watch::Sender<State>>
where
    S: RunnableService + 'static,
{
    let (sender, _) = watch::channel(State::NotStarted);
    let state = Shared::new(sender);
    let stop_sender = state.clone();
    // Spawned as a task to check if the service is already running and to capture any panics.
    tokio::task::spawn(async move {
        let join_handler = run(service, stop_sender.clone());
        let result = join_handler.await;

        let stopped_state = if let Err(e) = result {
            State::StoppedWithError(e.to_string())
        } else {
            State::Stopped
        };

        let _ = stop_sender.send_if_modified(|state| {
            if !state.stopped() {
                *state = stopped_state;
                true
            } else {
                false
            }
        });
    });
    state
}

/// Spawns a task for the main background run loop.
fn run<S>(service: S, sender: Shared<watch::Sender<State>>) -> JoinHandle<()>
where
    S: RunnableService + 'static,
{
    let mut state: StateWatcher = sender.subscribe().into();
    tokio::task::spawn(async move {
        if state.borrow_and_update().not_started() {
            // We can panic here, because it is inside of the task.
            state.changed().await.expect("The service is destroyed");
        }

        // If the state after update is not `Starting` then return to stop the service.
        if !state.borrow().starting() {
            return
        }

        // We can panic here, because it is inside of the task.
        let mut task = service
            .into_task(&state)
            .await
            .expect("The initialization of the service failed.");

        let started = sender.send_if_modified(|s| {
            if s.starting() {
                *s = State::Started;
                true
            } else {
                false
            }
        });

        if !started {
            return
        }

        while state.borrow_and_update().started() {
            match task.run(&mut state).await {
                Ok(should_continue) => {
                    if !should_continue {
                        return
                    }
                }
                Err(e) => {
                    let e: &dyn std::error::Error = &*e;
                    tracing::error!(e);
                }
            }
        }
    })
}

// TODO: Add tests
#[cfg(test)]
mod tests {
    use super::*;
    use futures::future::BoxFuture;

    mockall::mock! {
        Service {}

        #[async_trait::async_trait]
        impl RunnableService for Service {
            const NAME: &'static str = "MockService";

            type SharedData = EmptyShared;
            type Task = MockTask;

            fn shared_data(&self) -> EmptyShared;

            async fn into_task(self, state: &StateWatcher) -> anyhow::Result<MockTask>;
        }
    }

    mockall::mock! {
        Task {}

        #[async_trait::async_trait]
        impl RunnableTask for Task {
            fn run<'_self, '_state, 'a>(
                &'_self mut self,
                state: &'_state mut StateWatcher
            ) -> BoxFuture<'a, anyhow::Result<bool>>
            where
                '_self: 'a,
                '_state: 'a,
                Self: Sync + 'a;
        }
    }

    impl MockService {
        fn new_empty() -> Self {
            let mut mock = MockService::default();
            mock.expect_shared_data().returning(|| EmptyShared);
            mock.expect_into_task().returning(|_| {
                let mut mock = MockTask::default();
                mock.expect_run().returning(|watcher| {
                    let mut watcher = watcher.clone();
                    Box::pin(async move {
                        watcher.while_started().await.unwrap();
                        Ok(false)
                    })
                });
                Ok(mock)
            });
            mock
        }
    }

    #[tokio::test]
    async fn start_and_await_stop_and_await_works() {
        let service = ServiceRunner::new(MockService::new_empty());
        let state = service.start_and_await().await.unwrap();
        assert!(state.started());
        let state = service.stop_and_await().await.unwrap();
        assert!(state.stopped());
    }

    #[tokio::test]
    async fn double_start_fails() {
        let service = ServiceRunner::new(MockService::new_empty());
        assert!(service.start().is_ok());
        assert!(service.start().is_err());
    }

    #[tokio::test]
    async fn double_start_and_await_fails() {
        let service = ServiceRunner::new(MockService::new_empty());
        assert!(service.start_and_await().await.is_ok());
        assert!(service.start_and_await().await.is_err());
    }

    #[tokio::test]
    async fn stop_without_start() {
        let service = ServiceRunner::new(MockService::new_empty());
        service.stop_and_await().await.unwrap();
    }

    #[tokio::test]
    async fn panic_during_run() {
        let mut mock = MockService::default();
        mock.expect_shared_data().returning(|| EmptyShared);
        mock.expect_into_task().returning(|_| {
            let mut mock = MockTask::default();
            mock.expect_run().returning(|_| panic!("Should fail"));
            Ok(mock)
        });
        let service = ServiceRunner::new(mock);
        let state = service.start_and_await().await.unwrap();
        assert!(matches!(state, State::StoppedWithError(_)));

        let state = service.await_stop().await.unwrap();
        assert!(matches!(state, State::StoppedWithError(_)));
    }

    #[tokio::test]
    async fn double_await_stop_works() {
        let service = ServiceRunner::new(MockService::new_empty());
        service.start().unwrap();
        service.stop();

        let state = service.await_stop().await.unwrap();
        assert!(state.stopped());
        let state = service.await_stop().await.unwrap();
        assert!(state.stopped());
    }

    #[tokio::test]
    async fn double_stop_and_await_works() {
        let service = ServiceRunner::new(MockService::new_empty());
        service.start().unwrap();

        let state = service.stop_and_await().await.unwrap();
        assert!(state.stopped());
        let state = service.stop_and_await().await.unwrap();
        assert!(state.stopped());
    }
}
