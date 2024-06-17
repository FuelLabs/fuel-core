use crate::{
    state::{
        State,
        StateWatcher,
    },
    Shared,
};
use anyhow::{anyhow, Context};
use fuel_core_metrics::{
    future_tracker::FutureTracker,
    services::{
        services_metrics,
        ServiceLifecycle,
    },
};
use futures::FutureExt;
use std::any::Any;
use tokio::sync::watch;
use tracing::{error, info, debug, Instrument};

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
        let metric = services_metrics().register_service(S::NAME);
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
            start.changed().await.context("Failed to await state change")?;
        }
    }

    async fn _await_stop(&self, mut stop: StateWatcher) -> anyhow::Result<State> {
        loop {
            let state = stop.borrow().clone();
            if state.stopped() {
                return Ok(state);
            }
            stop.changed().await.context("Failed to await state change")?;
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
    metric: ServiceLifecycle,
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
            debug!("running");
            let run = std::panic::AssertUnwindSafe(run(
                service,
                state.clone(),
                params,
                metric.clone(),
            ));
            let res = run.catch_unwind().await;

            if res.is_err() {
                let mut state = state.borrow_mut();
                *state = State::Stopped;
            }
            metric.on_drop();
        }
        .instrument(tracing::info_span!("task")),
    );

    // Ensuring the stop signal is sent on drop.
    tokio::task::spawn(async move {
        stop_sender.closed().await;
        stop_sender.borrow_and_update();
        stop_sender.send(State::Stopped).ok();
    });

    state
}

#[tracing::instrument(skip_all, fields(service = S::NAME))]
/// Run the main loop for the service.
async fn run<S>(
    service: S,
    state: Shared<watch::Sender<State>>,
    params: S::TaskParams,
    metric: ServiceLifecycle,
) where
    S: RunnableService + 'static,
{
    let mut watcher: StateWatcher = state.subscribe().into();
    let mut task = match service.into_task(&watcher, params).await {
        Ok(task) => {
            state.send(State::Started).ok();
            metric.on_start();
            task
        }
        Err(e) => {
            error!(error = %e, "Failed to initialize service task");
            state.send(State::Stopped).ok();
            return;
        }
    };

    let task_res = loop {
        if watcher.stop_requested().await {
            break Ok(());
        }
        if let Err(e) = task.run(&mut watcher).await {
            error!(error = %e, "Service task encountered an error");
        }
    };

    match task_res {
        Ok(()) => {
            info!("Service task completed successfully");
        }
        Err(e) => {
            error!(error = %e, "Service task failed");
        }
    }

    task.shutdown().await.ok();
    state.send(State::Stopped).ok();
    metric.on_stop();
}
