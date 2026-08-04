use fuel_core_poa::ports::WaitForReadySignal;
use std::sync::{
    Arc,
    atomic::{
        AtomicBool,
        Ordering,
    },
};

#[derive(Clone)]
pub struct ReadySignal {
    notifier: Arc<tokio::sync::Notify>,
    is_ready: Arc<AtomicBool>,
}

impl WaitForReadySignal for ReadySignal {
    async fn wait_for_ready_signal(&self) {
        self.notifier.notified().await;
    }
}

impl ReadySignal {
    pub fn new() -> Self {
        Self {
            notifier: Arc::new(tokio::sync::Notify::new()),
            is_ready: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn send_ready_signal(&self) {
        self.is_ready.store(true, Ordering::Release);
        self.notifier.notify_one();
    }

    pub fn is_ready(&self) -> bool {
        self.is_ready.load(Ordering::Acquire)
    }
}

impl Default for ReadySignal {
    fn default() -> Self {
        Self::new()
    }
}
