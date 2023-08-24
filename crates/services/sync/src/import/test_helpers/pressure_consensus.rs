use crate::{
    import::test_helpers::counts::SharedCounts,
    ports::{
        ConsensusPort,
        MockConsensusPort,
    },
};
use fuel_core_types::blockchain::{
    primitives::DaBlockHeight,
    SealedBlockHeader,
};
use std::time::Duration;

pub struct PressureConsensus(MockConsensusPort, Duration, SharedCounts);

#[async_trait::async_trait]
impl ConsensusPort for PressureConsensus {
    fn check_sealed_header(&self, header: &SealedBlockHeader) -> anyhow::Result<bool> {
        self.0.check_sealed_header(header)
    }

    async fn await_da_height(&self, da_height: &DaBlockHeight) -> anyhow::Result<()> {
        self.2.apply(|c| c.inc_consensus());
        tokio::time::sleep(self.1).await;
        self.2.apply(|c| c.dec_consensus());
        self.0.await_da_height(da_height).await
    }
}

impl PressureConsensus {
    pub fn new(counts: SharedCounts, delays: Duration) -> Self {
        let mut mock = MockConsensusPort::default();
        mock.expect_await_da_height().returning(|_| Ok(()));
        mock.expect_check_sealed_header().returning(|_| Ok(true));
        Self(mock, delays, counts)
    }
}
