use anyhow::anyhow;
use tokio::{
    sync::watch,
    task::JoinHandle,
};

pub type Shared<T> = std::sync::Arc<T>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmptyShared;

#[async_trait::async_trait]
pub trait Service {
    fn start(&self) -> anyhow::Result<()>;

    fn stop(&self) -> bool;

    async fn stop_and_await(&self) -> anyhow::Result<State>;

    async fn await_stop(&self) -> anyhow::Result<State>;

    fn state(&self) -> State;
}

#[async_trait::async_trait]
pub trait RunnableService: Send + Sync {
    const NAME: &'static str;

    type SharedData: Clone + Send + Sync;

    fn shared_data(&self) -> Self::SharedData;

    async fn initialize(&mut self) -> anyhow::Result<()>;

    /// `ServiceRunner` calls `run` function until it returns `false`.
    async fn run(&mut self) -> anyhow::Result<bool>;
}

#[derive(Debug, Clone)]
pub enum State {
    NotStarted,
    Started,
    Stopping,
    Stopped,
    StoppedWithError(String),
}

impl State {
    pub fn not_started(&self) -> bool {
        self == &State::NotStarted
    }

    pub fn started(&self) -> bool {
        self == &State::Started
    }

    pub fn stopped(&self) -> bool {
        matches!(self, State::Stopped | State::StoppedWithError(_))
    }
}

impl PartialEq for State {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::NotStarted, Self::NotStarted) => true,
            (Self::Started, Self::Started) => true,
            (Self::Stopping, Self::Stopping) => true,
            (Self::Stopped, Self::Stopped) => true,
            (Self::StoppedWithError(_), Self::StoppedWithError(_)) => true,
            (_, _) => false,
        }
    }
}

#[derive(Debug)]
pub struct ServiceRunner<S>
where
    S: RunnableService,
{
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

/// Initialize the background loop.
fn initialize_loop<S>(service: S) -> Shared<watch::Sender<State>>
where
    S: RunnableService + 'static,
{
    let (sender, receiver) = watch::channel(State::NotStarted);
    let state = Shared::new(sender);
    let stop_sender = state.clone();
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

/// Main background run loop.
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
}
