use std::time::Duration;

use fuel_core_types::tai64::Tai64N;
use tokio::sync::watch;

use crate::ports::GetTime;

pub struct TestTime {
    time: watch::Sender<Tai64N>,
}

impl TestTime {
    pub fn at_unix_epoch() -> Self {
        Self::new(Tai64N::UNIX_EPOCH)
    }

    pub fn new(start_time: Tai64N) -> Self {
        tokio::time::pause();
        let time = watch::Sender::new(start_time);

        Self { time }
    }

    pub fn advance(&mut self, duration: Duration) {
        self.time
            .send_modify(|timestamp| *timestamp = *timestamp + duration);
    }

    pub fn watch(&self) -> Watch {
        let time = self.time.subscribe();
        Watch { time }
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
    fn now(&self) -> fuel_core_types::tai64::Tai64 {
        self.time.borrow().0
    }
}
