use fuel_core_poa::ports::WaitForReadySignal;
use std::sync::Arc;

#[derive(Clone)]
pub struct ReadySignal {
    notifier: Arc<tokio::sync::Notify>,
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
        }
    }

    pub fn send_ready_signal(&self) {
        self.notifier.notify_one();
    }
}

impl Default for ReadySignal {
    fn default() -> Self {
        Self::new()
    }
}
