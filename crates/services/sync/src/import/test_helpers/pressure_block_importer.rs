use crate::{
    import::test_helpers::SharedCounts,
    ports::{
        BlockImporterPort,
        MockBlockImporterPort,
    },
};
use fuel_core_services::stream::BoxStream;
use fuel_core_types::{
    blockchain::SealedBlock,
    fuel_types::BlockHeight,
};
use std::time::Duration;

pub struct PressureBlockImporter(MockBlockImporterPort, Duration, SharedCounts);

#[async_trait::async_trait]
impl BlockImporterPort for PressureBlockImporter {
    fn committed_height_stream(&self) -> BoxStream<BlockHeight> {
        self.0.committed_height_stream()
    }

    async fn execute_and_commit(&self, block: SealedBlock) -> anyhow::Result<()> {
        self.2.apply(|c| c.inc_executes());
        tokio::time::sleep(self.1).await;
        self.2.apply(|c| {
            c.dec_executes();
            c.dec_blocks();
        });
        self.0.execute_and_commit(block).await
    }
}

impl PressureBlockImporter {
    pub fn new(counts: SharedCounts, delays: Duration) -> Self {
        let mut mock = MockBlockImporterPort::default();
        mock.expect_execute_and_commit().returning(move |_| Ok(()));
        Self(mock, delays, counts)
    }
}
