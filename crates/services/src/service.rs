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
    /// Send a start signal to the `ServiceRunner`. Returns an error if the service was already
    /// started.
    fn start(&self) -> anyhow::Result<()>;

    /// Send a stop signal to the service without waiting for it to shutdown.
    /// Returns false if the service was already stopped, true if it is running.
    fn stop(&self) -> bool;

    /// Send stop signal to service and wait for it to shutdown.
    async fn stop_and_await(&self) -> anyhow::Result<State>;

    /// Wait for service to stop (without sending a stop signal)
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

    /// A cloned instance of the shared data
    fn shared_data(&self) -> Self::SharedData;

    /// Service specific initialization logic before main task run loop.
    async fn initialize(&mut self) -> anyhow::Result<()>;

    /// This function should contain the main business logic of the service. It will run until the
    /// service either returns false, panics or a stop signal is received.
    /// If the service returns an error, it will be logged and execution will resume.
    /// This is intended to be called only by the `ServiceRunner`.
    async fn run(&mut self) -> anyhow::Result<bool>;
}

/// The lifecycle state of the service
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    /// Service is initialized but not started
    NotStarted,
    /// Service is running as normal
    Started,
    /// Service is shutting down
    Stopping,
    /// Service is stopped
    Stopped,
    /// Service shutdown due to an error (panic)
    StoppedWithError(String),
}

impl State {
    /// is not started
    pub fn not_started(&self) -> bool {
        self == &State::NotStarted
    }

    /// is started
    pub fn started(&self) -> bool {
        self == &State::Started
    }

    /// is stopped
    pub fn stopped(&self) -> bool {
        matches!(self, State::Stopped | State::StoppedWithError(_))
    }
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

    async fn await_stop_and_maybe_call<F>(&self, call: F) -> anyhow::Result<State>
    where
        F: Fn(&Self),
    {
        let mut stop = self.state.subscribe();
        let state = stop.borrow().clone();
        if state.stopped() {
            Ok(state)
        } else {
            call(self);

            loop {
                let state = stop.borrow_and_update().clone();
                if state.stopped() {
                    return Ok(state)
                }
                stop.changed().await?;
            }
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
                *state = State::Started;
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

    fn stop(&self) -> bool {
        self.state.send_if_modified(|state| {
            if state.not_started() || state.started() {
                *state = State::Stopping;
                true
            } else {
                false
            }
        })
    }

    async fn stop_and_await(&self) -> anyhow::Result<State> {
        self.await_stop_and_maybe_call(|runner| {
            runner.stop();
        })
        .await
    }

    async fn await_stop(&self) -> anyhow::Result<State> {
        self.await_stop_and_maybe_call(|_| {}).await
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
    let (sender, receiver) = watch::channel(State::NotStarted);
    let state = Shared::new(sender);
    let stop_sender = state.clone();
    // Spawned as a task to check if the service is already running and to capture any panics.
    tokio::task::spawn(async move {
        let join_handler = run(service, receiver.clone());
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
fn run<S>(mut service: S, mut state: watch::Receiver<State>) -> JoinHandle<()>
where
    S: RunnableService + 'static,
{
    tokio::task::spawn(async move {
        if state.borrow_and_update().not_started() {
            // We can panic here, because it is inside of the task.
            state.changed().await.expect("The service is destroyed");
        }

        // If the state after update is not `Started` then return to stop the service.
        if !state.borrow().started() {
            return
        }

        // We can panic here, because it is inside of the task.
        service
            .initialize()
            .await
            .expect("The initialization of the service failed.");
        loop {
            tokio::select! {
                biased;

                _ = state.changed() => {
                    if !state.borrow_and_update().started() {
                        return
                    }
                }

                result = service.run() => {
                    match result {
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
            }
        }
    })
}

// TODO: Add tests
#[cfg(test)]
mod tests {
    use crate::{
        EmptyShared,
        RunnableService,
        Service as ServiceTrait,
        ServiceRunner,
        State,
    };

    mockall::mock! {
        Service {}

        #[async_trait::async_trait]
        impl RunnableService for Service {
            const NAME: &'static str = "MockService";

            type SharedData = EmptyShared;

            fn shared_data(&self) -> EmptyShared {
                EmptyShared
            }

            async fn initialize(&mut self) -> anyhow::Result<()> {
                Ok(())
            }

            async fn run(&mut self) -> anyhow::Result<bool>;
        }
    }

    impl MockService {
        fn new_empty() -> Self {
            let mut mock = MockService::default();
            mock.expect_shared_data().returning(|| EmptyShared);
            mock.expect_initialize().returning(|| Ok(()));
            mock.expect_run().returning(|| Ok(true));
            mock
        }
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
        mock.expect_initialize().returning(|| Ok(()));
        mock.expect_run().returning(|| panic!("Should fail"));
        let service = ServiceRunner::new(mock);
        service.start().unwrap();

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
