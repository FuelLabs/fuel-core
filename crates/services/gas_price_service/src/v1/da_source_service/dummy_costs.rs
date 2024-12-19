use crate::v1::da_source_service::{
    service::{
        DaBlockCostsSource,
        Result as DaBlockCostsResult,
    },
    DaBlockCosts,
};
use fuel_core_types::fuel_types::BlockHeight;
use std::sync::Arc;
use tokio::sync::Notify;

pub struct DummyDaBlockCosts {
    value: DaBlockCostsResult<DaBlockCosts>,
    // This is a workaround to notify the test when the value is ready/errored.
    notifier: Arc<Notify>,
}

impl DummyDaBlockCosts {
    pub fn new(value: DaBlockCostsResult<DaBlockCosts>, notifier: Arc<Notify>) -> Self {
        Self { value, notifier }
    }
}

#[async_trait::async_trait]
impl DaBlockCostsSource for DummyDaBlockCosts {
    async fn request_da_block_costs(&mut self) -> DaBlockCostsResult<Vec<DaBlockCosts>> {
        match &self.value {
            Ok(da_block_costs) => {
                self.notifier.notify_waiters();
                Ok(vec![da_block_costs.clone()])
            }
            Err(err) => {
                self.notifier.notify_waiters();
                Err(anyhow::anyhow!(err.to_string()))
            }
        }
    }

    async fn set_last_value(&mut self, _height: BlockHeight) -> DaBlockCostsResult<()> {
        unimplemented!("This is a dummy implementation");
    }
}
