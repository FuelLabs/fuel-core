use fuel_core_types::fuel_types::BlockHeight;
use std::sync::Arc;
use tokio::sync::{
    watch,
    Mutex,
};

#[derive(Default)]
pub struct Handler {
    inner: watch::Sender<BlockHeight>,
}

impl Handler {
    pub fn new(block_height: BlockHeight) -> Handler {
        Self {
            inner: watch::channel(block_height).0,
        }
    }

    pub fn subscribe(&self) -> Subscriber {
        Subscriber {
            inner: Arc::new(Mutex::new(self.inner.subscribe())),
        }
    }

    pub fn notify_and_update(&self, block_height: BlockHeight) -> anyhow::Result<()> {
        self.inner.send(block_height)?;
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct Subscriber {
    inner: Arc<Mutex<watch::Receiver<BlockHeight>>>,
}

impl Subscriber {
    pub async fn wait_for_block_height(
        &self,
        block_height: BlockHeight,
    ) -> anyhow::Result<()> {
        self.inner
            .lock()
            .await
            .wait_for(|seen_height| *seen_height >= block_height)
            .await?;

        Ok(())
    }

    pub async fn latest_seen_block_height(&self) -> BlockHeight {
        *self.inner.lock().await.borrow()
    }
}
