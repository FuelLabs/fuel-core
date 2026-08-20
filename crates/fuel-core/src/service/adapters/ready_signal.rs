use fuel_core_poa::{
    ports::WaitForReadySignal,
    service::SharedState as PoASharedState,
};
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

/// Bundle for `/v1/ready`: services-started flag plus optional PoA readiness.
/// PoA owns Ready via `SyncTask` (`height_gap_is_ready` + `time_until_synced`).
#[derive(Clone)]
pub struct Readiness {
    services_started: ReadySignal,
    poa: Option<PoASharedState>,
}

impl Readiness {
    pub fn new(services_started: ReadySignal, poa: Option<PoASharedState>) -> Self {
        Self {
            services_started,
            poa,
        }
    }

    pub fn services_started(&self) -> bool {
        self.services_started.is_ready()
    }

    pub fn poa_enabled(&self) -> bool {
        self.poa.is_some()
    }

    /// `true` when PoA is disabled, or when PoA reports Ready.
    pub async fn poa_ready(&self) -> bool {
        match &self.poa {
            Some(poa) => poa.is_ready().await,
            None => true,
        }
    }

    pub async fn is_ready(&self) -> bool {
        self.services_started() && self.poa_ready().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_ready_is_false_before_send_ready_signal_and_true_after() {
        let signal = ReadySignal::new();

        assert!(!signal.is_ready());

        signal.send_ready_signal();

        assert!(signal.is_ready());
    }

    #[tokio::test]
    async fn readiness_poa_disabled_is_ready_once_services_started() {
        let signal = ReadySignal::new();
        let readiness = Readiness::new(signal.clone(), None);

        assert!(!readiness.is_ready().await);
        signal.send_ready_signal();
        assert!(readiness.is_ready().await);
        assert!(!readiness.poa_enabled());
        assert!(readiness.poa_ready().await);
    }
}
