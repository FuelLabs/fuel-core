use crate::{
    state::{
        State,
        StateWatcher,
    },
    Shared,
};
use anyhow::anyhow;
use fuel_core_metrics::{
    future_tracker::FutureTracker,
    services::ServicesMetrics,
};
use futures::FutureExt;
use std::any::Any;
use tokio::sync::watch;
use tracing::Instrument;

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

    /// Returns the state watcher of the service.
    fn state_watcher(&self) -> StateWatcher;
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

    /// Optional parameters used to when initializing into task.
    type TaskParams: Send;

    /// A cloned instance of the shared data
    fn shared_data(&self) -> Self::SharedData;

    /// Converts the service into a runnable task before the main run loop.
    ///
    /// The `state` is a `State` watcher of the service. Some tasks may handle state changes
    /// on their own.
    async fn into_task(
        self,
        state_watcher: &StateWatcher,
        params: Self::TaskParams,
    ) -> anyhow::Result<Self::Task>;
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

    /// Gracefully shutdowns the task after the end of the execution cycle.
    async fn shutdown(self) -> anyhow::Result<()>;
}

/// The service runner manages the lifecycle, execution and error handling of a `RunnableService`.
/// It can be cloned and passed between threads.
#[derive(Debug)]
pub struct ServiceRunner<S>
where
    S: RunnableService + 'static,
{
    /// The shared state of the service
    pub shared: S::SharedData,
    state: Shared<watch::Sender<State>>,
}

impl<S> Drop for ServiceRunner<S>
where
    S: RunnableService + 'static,
{
    fn drop(&mut self) {
        self.stop();
    }
}

impl<S> ServiceRunner<S>
where
    S: RunnableService + 'static,
    S::TaskParams: Default,
{
    /// Initializes a new `ServiceRunner` containing a `RunnableService`
    pub fn new(service: S) -> Self {
        Self::new_with_params(service, S::TaskParams::default())
    }
}

impl<S> ServiceRunner<S>
where
    S: RunnableService + 'static,
{
    /// Initializes a new `ServiceRunner` containing a `RunnableService` with parameters for underlying `Task`
    pub fn new_with_params(service: S, params: S::TaskParams) -> Self {
        let shared = service.shared_data();
        let metric = ServicesMetrics::register_service(S::NAME);
        let state = initialize_loop(service, params, metric);
        Self { shared, state }
    }

    async fn _await_start_or_stop(
        &self,
        mut start: StateWatcher,
    ) -> anyhow::Result<State> {
        loop {
            let state = start.borrow().clone();
            if !state.starting() {
                return Ok(state);
            }
            start.changed().await?;
        }
    }

    async fn _await_stop(&self, mut stop: StateWatcher) -> anyhow::Result<State> {
        loop {
            let state = stop.borrow().clone();
            if state.stopped() {
                return Ok(state);
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

    fn state_watcher(&self) -> StateWatcher {
        self.state.subscribe().into()
    }
}

#[tracing::instrument(skip_all, fields(service = S::NAME))]
/// Initialize the background loop as a spawned task.
fn initialize_loop<S>(
    service: S,
    params: S::TaskParams,
    metric: ServicesMetrics,
) -> Shared<watch::Sender<State>>
where
    S: RunnableService + 'static,
{
    let (sender, _) = watch::channel(State::NotStarted);
    let state = Shared::new(sender);
    let stop_sender = state.clone();
    // Spawned as a task to check if the service is already running and to capture any panics.
    tokio::task::spawn(
        async move {
            tracing::debug!("running");
            let run = std::panic::AssertUnwindSafe(run(
                service,
                stop_sender.clone(),
                params,
                metric,
            ));
            tracing::debug!("awaiting run");
            let result = run.catch_unwind().await;

            let stopped_state = if let Err(e) = result {
                let panic_information = panic_to_string(e);
                State::StoppedWithError(panic_information)
            } else {
                State::Stopped
            };

            tracing::debug!("shutting down {:?}", stopped_state);

            let _ = stop_sender.send_if_modified(|state| {
                if !state.stopped() {
                    *state = stopped_state.clone();
                    tracing::debug!("Wasn't stopped, so sent stop.");
                    true
                } else {
                    tracing::debug!("Was already stopped.");
                    false
                }
            });

            tracing::info!("The service {} is shut down", S::NAME);

            if let State::StoppedWithError(err) = stopped_state {
                std::panic::resume_unwind(Box::new(err));
            }
        }
        .in_current_span(),
    );
    state
}

/// Runs the main loop.
async fn run<S>(
    service: S,
    sender: Shared<watch::Sender<State>>,
    params: S::TaskParams,
    metric: ServicesMetrics,
) where
    S: RunnableService + 'static,
{
    let mut state: StateWatcher = sender.subscribe().into();
    if state.borrow_and_update().not_started() {
        // We can panic here, because it is inside of the task.
        state.changed().await.expect("The service is destroyed");
    }

    // If the state after update is not `Starting` then return to stop the service.
    if !state.borrow().starting() {
        return;
    }

    // We can panic here, because it is inside of the task.
    tracing::info!("Starting {} service", S::NAME);
    let mut task = service
        .into_task(&state, params)
        .await
        .expect("The initialization of the service failed.");

    sender.send_if_modified(|s| {
        if s.starting() {
            *s = State::Started;
            true
        } else {
            false
        }
    });

    let got_panic = run_task(&mut task, state, &metric).await;

    let got_panic = shutdown_task(S::NAME, task, got_panic).await;

    if let Some(panic) = got_panic {
        std::panic::resume_unwind(panic)
    }
}

async fn run_task<S: RunnableTask>(
    task: &mut S,
    mut state: StateWatcher,
    metric: &ServicesMetrics,
) -> Option<Box<dyn Any + Send>> {
    let mut got_panic = None;

    while state.borrow_and_update().started() {
        let tracked_task = FutureTracker::new(task.run(&mut state));
        let task = std::panic::AssertUnwindSafe(tracked_task);
        let panic_result = task.catch_unwind().await;

        if let Err(panic) = panic_result {
            tracing::debug!("got a panic");
            got_panic = Some(panic);
            break;
        }

        let tracked_result = panic_result.expect("Checked the panic above");

        // TODO: Use `u128` when `AtomicU128` is stable.
        metric.busy.inc_by(
            u64::try_from(tracked_result.busy.as_nanos())
                .expect("The task doesn't live longer than `u64`"),
        );
        metric.idle.inc_by(
            u64::try_from(tracked_result.idle.as_nanos())
                .expect("The task doesn't live longer than `u64`"),
        );

        let result = tracked_result.output;

        match result {
            Ok(should_continue) => {
                if !should_continue {
                    tracing::debug!("stopping");
                    break;
                }
                tracing::debug!("run loop");
            }
            Err(e) => {
                let e: &dyn std::error::Error = &*e;
                tracing::error!(e);
            }
        }
    }
    got_panic
}

async fn shutdown_task<S>(
    name: &str,
    task: S,
    mut got_panic: Option<Box<dyn Any + Send>>,
) -> Option<Box<dyn Any + Send>>
where
    S: RunnableTask,
{
    tracing::info!("Shutting down {} service", name);
    let shutdown = std::panic::AssertUnwindSafe(task.shutdown());
    match shutdown.catch_unwind().await {
        Ok(Ok(_)) => {}
        Ok(Err(e)) => {
            tracing::error!("Go an error during shutdown of the task: {e}");
        }
        Err(e) => {
            if got_panic.is_some() {
                let panic_information = panic_to_string(e);
                tracing::error!(
                    "Go a panic during execution and shutdown of the task. \
                    The error during shutdown: {panic_information}"
                );
            } else {
                got_panic = Some(e);
            }
        }
    }
    got_panic
}

fn panic_to_string(e: Box<dyn core::any::Any + Send>) -> String {
    match e.downcast::<String>() {
        Ok(v) => *v,
        Err(e) => match e.downcast::<&str>() {
            Ok(v) => v.to_string(),
            _ => "Unknown Source of Error".to_owned(),
        },
    }
}

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
            type TaskParams = ();

            fn shared_data(&self) -> EmptyShared;

            async fn into_task(self, state: &StateWatcher, params: <MockService as RunnableService>::TaskParams) -> anyhow::Result<MockTask>;
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

            async fn shutdown(self) -> anyhow::Result<()>;
        }
    }

    impl MockService {
        fn new_empty() -> Self {
            let mut mock = MockService::default();
            mock.expect_shared_data().returning(|| EmptyShared);
            mock.expect_into_task().returning(|_, _| {
                let mut mock = MockTask::default();
                mock.expect_run().returning(|watcher| {
                    let mut watcher = watcher.clone();
                    Box::pin(async move {
                        watcher.while_started().await.unwrap();
                        let should_continue = false;
                        Ok(should_continue)
                    })
                });
                mock.expect_shutdown().times(1).returning(|| Ok(()));
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
        assert!(matches!(state, State::Stopped));
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
        assert!(matches!(service.state(), State::Stopped));
    }

    #[tokio::test]
    async fn panic_during_run() {
        let mut mock = MockService::default();
        mock.expect_shared_data().returning(|| EmptyShared);
        mock.expect_into_task().returning(|_, _| {
            let mut mock = MockTask::default();
            mock.expect_run().returning(|_| panic!("Should fail"));
            mock.expect_shutdown().times(1).returning(|| Ok(()));
            Ok(mock)
        });
        let service = ServiceRunner::new(mock);
        let state = service.start_and_await().await.unwrap();
        assert!(matches!(state, State::StoppedWithError(s) if s.contains("Should fail")));

        let state = service.await_stop().await.unwrap();
        assert!(matches!(state, State::StoppedWithError(s) if s.contains("Should fail")));
    }

    #[tokio::test]
    async fn panic_during_shutdown() {
        let mut mock = MockService::default();
        mock.expect_shared_data().returning(|| EmptyShared);
        mock.expect_into_task().returning(|_, _| {
            let mut mock = MockTask::default();
            mock.expect_run().returning(|_| {
                Box::pin(async move {
                    let should_continue = false;
                    Ok(should_continue)
                })
            });
            mock.expect_shutdown()
                .times(1)
                .returning(|| panic!("Shutdown should fail"));
            Ok(mock)
        });
        let service = ServiceRunner::new(mock);
        let state = service.start_and_await().await.unwrap();
        assert!(
            matches!(state, State::StoppedWithError(s) if s.contains("Shutdown should fail"))
        );

        let state = service.await_stop().await.unwrap();
        assert!(
            matches!(state, State::StoppedWithError(s) if s.contains("Shutdown should fail"))
        );
    }

    #[tokio::test]
    async fn double_await_stop_works() {
        let service = ServiceRunner::new(MockService::new_empty());
        service.start().unwrap();
        service.stop();

        let state = service.await_stop().await.unwrap();
        assert!(matches!(state, State::Stopped));
        let state = service.await_stop().await.unwrap();
        assert!(matches!(state, State::Stopped));
    }

    #[tokio::test]
    async fn double_stop_and_await_works() {
        let service = ServiceRunner::new(MockService::new_empty());
        service.start().unwrap();

        let state = service.stop_and_await().await.unwrap();
        assert!(matches!(state, State::Stopped));
        let state = service.stop_and_await().await.unwrap();
        assert!(matches!(state, State::Stopped));
    }

    #[tokio::test]
    async fn stop_unused_service() {
        let mut receiver;
        {
            let service = ServiceRunner::new(MockService::new_empty());
            service.start().unwrap();
            receiver = service.state.subscribe();
        }

        receiver.changed().await.unwrap();
        assert!(matches!(receiver.borrow().clone(), State::Stopping));
        receiver.changed().await.unwrap();
        assert!(matches!(receiver.borrow().clone(), State::Stopped));
    }
}
