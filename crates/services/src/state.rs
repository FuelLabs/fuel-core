//! The module related to state of the service.

use tokio::sync::watch;

/// The lifecycle state of the service
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    /// Service is initialized but not started
    NotStarted,
    /// Service is starting up
    Starting,
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

    /// is starting
    pub fn starting(&self) -> bool {
        self == &State::Starting
    }

    /// is started
    pub fn started(&self) -> bool {
        self == &State::Started
    }

    /// is stopped
    pub fn stopped(&self) -> bool {
        matches!(self, State::Stopped | State::StoppedWithError(_))
    }

    /// is stopping
    pub fn stopping(&self) -> bool {
        self == &State::Stopping
    }
}

/// The wrapper around the `watch::Receiver<State>`. It repeats the `Receiver` functionality +
/// a new one.
#[derive(Clone)]
pub struct StateWatcher(watch::Receiver<State>);

#[cfg(feature = "test-helpers")]
impl Default for StateWatcher {
    fn default() -> Self {
        let (_, receiver) = watch::channel(State::NotStarted);
        Self(receiver)
    }
}

#[cfg(feature = "test-helpers")]
impl StateWatcher {
    /// Create a new `StateWatcher` with the `State::Started` state.
    pub fn started() -> Self {
        let (sender, receiver) = watch::channel(State::Started);
        // This function is used only in tests, so for simplicity of the tests, we want to leak sender.
        core::mem::forget(sender);
        Self(receiver)
    }
}

impl StateWatcher {
    /// See [`watch::Receiver::borrow`].
    pub fn borrow(&self) -> watch::Ref<'_, State> {
        self.0.borrow()
    }

    /// See [`watch::Receiver::borrow_and_update`].
    pub fn borrow_and_update(&mut self) -> watch::Ref<'_, State> {
        self.0.borrow_and_update()
    }

    /// See [`watch::Receiver::has_changed`].
    pub fn has_changed(&self) -> Result<bool, watch::error::RecvError> {
        self.0.has_changed()
    }

    /// See [`watch::Receiver::changed`].
    pub async fn changed(&mut self) -> Result<(), watch::error::RecvError> {
        self.0.changed().await
    }

    /// See [`watch::Receiver::same_channel`].
    pub fn same_channel(&self, other: &Self) -> bool {
        self.0.same_channel(&other.0)
    }
}

impl StateWatcher {
    #[tracing::instrument(level = "debug", skip(self), err, ret)]
    /// Infinity loop while the state is `State::Started`. Returns the next received state.
    pub async fn while_started(&mut self) -> anyhow::Result<State> {
        loop {
            let state = self.borrow().clone();
            if !state.started() {
                return Ok(state);
            }

            self.changed().await?;
        }
    }

    /// Future that resolves once the state is `State::Stopped`.
    pub async fn wait_stopping_or_stopped(&mut self) -> anyhow::Result<()> {
        let state = self.borrow().clone();
        while !(state.stopped() || state.stopping()) {
            self.changed().await?;
        }
        Ok(())
    }
}

impl From<watch::Receiver<State>> for StateWatcher {
    fn from(receiver: watch::Receiver<State>) -> Self {
        Self(receiver)
    }
}
