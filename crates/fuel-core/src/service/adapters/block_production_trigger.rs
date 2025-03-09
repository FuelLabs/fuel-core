use fuel_core_poa::ports::TriggerBlockProduction;
use std::sync::Arc;

#[derive(Clone)]
pub struct BlockProductionTrigger {
    notifier: Arc<tokio::sync::Notify>,
}

impl TriggerBlockProduction for BlockProductionTrigger {
    async fn wait_for_trigger(&self) {
        self.notifier.notified().await;
    }
}

impl BlockProductionTrigger {
    pub fn new() -> Self {
        Self {
            notifier: Arc::new(tokio::sync::Notify::new()),
        }
    }

    pub fn send_trigger(&self) {
        self.notifier.notify_one();
    }
}

impl Default for BlockProductionTrigger {
    fn default() -> Self {
        Self::new()
    }
}
