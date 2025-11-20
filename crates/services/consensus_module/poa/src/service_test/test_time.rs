use std::time::Duration;

use fuel_core_types::tai64::{Tai64, Tai64N};
use tokio::{sync::watch, time::Instant};

use crate::ports::GetTime;

pub struct TestTime {
    time: watch::Sender<Tai64N>,
    last_observed_tokio_time: Instant,
}

impl TestTime {
    pub fn at_unix_epoch() -> Self {
        Self::new(Tai64N::UNIX_EPOCH)
    }

    pub fn new(start_time: Tai64N) -> Self {
        tokio::time::pause();
        let time = watch::Sender::new(start_time);
        let last_observed_tokio_time = Instant::now();

        Self {
            time,
            last_observed_tokio_time,
        }
    }

    /// Advances time by the same duration that Tokio's time has been advanced.
    ///
    /// This function is particularly useful in scenarios where `tokio::time::advance` has been
    /// manually invoked, or when Tokio has automatically advanced the time.
    /// For more information on auto-advancement, see the
    /// [Tokio documentation](https://docs.rs/tokio/1.39.3/tokio/time/fn.advance.html#auto-advance).
    pub fn advance_with_tokio(&mut self) {
        let current_tokio_time = Instant::now();

        let tokio_advance =
            current_tokio_time.duration_since(self.last_observed_tokio_time);
        self.last_observed_tokio_time = current_tokio_time;

        self.advance(tokio_advance);
    }

    pub fn advance(&mut self, duration: Duration) {
        self.time
            .send_modify(|timestamp| *timestamp = *timestamp + duration);
    }

    pub fn rewind(&mut self, duration: Duration) {
        self.time
            .send_modify(|timestamp| *timestamp = *timestamp - duration);
    }

    pub fn watch(&self) -> Watch {
        let time = self.time.subscribe();
        Watch { time }
    }
}

impl Default for TestTime {
    fn default() -> Self {
        Self::at_unix_epoch()
    }
}

impl Drop for TestTime {
    fn drop(&mut self) {
        tokio::time::resume();
    }
}

pub struct Watch {
    time: watch::Receiver<Tai64N>,
}

impl GetTime for Watch {
    fn now(&self) -> Tai64 {
        self.time.borrow().0
    }
}
