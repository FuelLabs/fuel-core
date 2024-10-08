use crate::v1::da_source_service::{
    service::{
        DaBlockCostsSource,
        Result as DaBlockCostsResult,
    },
    DaBlockCosts,
};
use tokio::sync::mpsc::Sender;

pub struct DummyDaBlockCosts {
    value: DaBlockCostsResult<DaBlockCosts>,
    // sends true if the request was successful, false otherwise
    notifier: Sender<bool>,
}

impl DummyDaBlockCosts {
    pub fn new(value: DaBlockCostsResult<DaBlockCosts>, notifier: Sender<bool>) -> Self {
        Self { value, notifier }
    }
}

#[async_trait::async_trait]
impl DaBlockCostsSource for DummyDaBlockCosts {
    async fn request_da_block_cost(&mut self) -> DaBlockCostsResult<DaBlockCosts> {
        match &self.value {
            Ok(da_block_costs) => {
                self.notifier.send(true).await?;
                Ok(da_block_costs.clone())
            }
            Err(err) => {
                self.notifier.send(false).await?;
                Err(anyhow::anyhow!(err.to_string()))
            }
        }
    }
}
