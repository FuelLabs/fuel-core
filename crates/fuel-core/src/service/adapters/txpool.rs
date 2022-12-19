use async_trait::async_trait;
use fuel_core_txpool::ports::BlockImport;
use fuel_core_types::blockchain::SealedBlock;
use tokio::sync::broadcast::Receiver;

/// This is used to get block import events from coordinator source
/// and pass them to the txpool.
pub struct BlockImportAdapter {
    rx: Receiver<SealedBlock>,
}
impl BlockImportAdapter {
    pub fn new(rx: Receiver<SealedBlock>) -> Self {
        Self { rx }
    }
}

#[async_trait]
impl BlockImport for BlockImportAdapter {
    async fn next_block(&mut self) -> SealedBlock {
        match self.rx.recv().await {
            Ok(block) => return block,
            Err(err) => {
                panic!("Block import channel errored unexpectedly: {err:?}");
            }
        }
    }
}
