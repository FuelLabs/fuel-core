use fuel_core_sync::ports::{
    ConsensusPort,
    MockConsensusPort,
};
use fuel_core_types::blockchain::{
    primitives::DaBlockHeight,
    SealedBlockHeader,
};
use std::time::Duration;

pub struct PressureConsensusPort(MockConsensusPort, Duration);

impl PressureConsensusPort {
    pub fn new(delays: Duration) -> Self {
        let mut mock = MockConsensusPort::default();
        mock.expect_await_da_height().returning(|_| Ok(()));
        mock.expect_check_sealed_header().returning(|_| Ok(true));
        Self(mock, delays)
    }

    fn service(&self) -> &impl ConsensusPort {
        &self.0
    }

    fn duration(&self) -> Duration {
        self.1
    }
}

#[async_trait::async_trait]
impl ConsensusPort for PressureConsensusPort {
    fn check_sealed_header(&self, header: &SealedBlockHeader) -> anyhow::Result<bool> {
        self.service().check_sealed_header(header)
    }

    async fn await_da_height(&self, da_height: &DaBlockHeight) -> anyhow::Result<()> {
        tokio::time::sleep(self.duration()).await;
        self.service().await_da_height(da_height).await
    }
}
