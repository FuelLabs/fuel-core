use fuel_core_services::stream::BoxStream;
use fuel_core_sync::ports::{
    BlockImporterPort,
    MockBlockImporterPort,
};
use fuel_core_types::{
    blockchain::SealedBlock,
    fuel_types::BlockHeight,
};
use std::time::Duration;

pub struct PressureBlockImporterPort(MockBlockImporterPort, Duration);

impl PressureBlockImporterPort {
    pub fn new(delays: Duration) -> Self {
        let mut mock = MockBlockImporterPort::default();
        mock.expect_execute_and_commit().returning(move |_| Ok(()));
        Self(mock, delays)
    }

    fn service(&self) -> &impl BlockImporterPort {
        &self.0
    }

    fn duration(&self) -> Duration {
        self.1
    }
}

#[async_trait::async_trait]
impl BlockImporterPort for PressureBlockImporterPort {
    fn committed_height_stream(&self) -> BoxStream<BlockHeight> {
        self.service().committed_height_stream()
    }

    async fn execute_and_commit(&self, block: SealedBlock) -> anyhow::Result<()> {
        let timeout = self.duration();
        tokio::task::spawn_blocking(move || {
            std::thread::sleep(timeout);
        })
        .await
        .unwrap();
        self.service().execute_and_commit(block).await
    }
}
