use std::{
    cmp::Reverse,
    collections::BTreeMap,
    sync::Arc,
};

use fuel_core_types::fuel_types::BlockHeight;
use parking_lot::RwLock;
use tokio::sync::oneshot;

#[derive(Default)]
pub struct Handler {
    inner: Arc<RwLock<HandlersMapInner>>,
}

impl Handler {
    pub fn new(block_height: BlockHeight) -> Handler {
        Self {
            inner: Arc::new(RwLock::new(HandlersMapInner::new(block_height))),
        }
    }

    pub fn subscribe(&self) -> Subscriber {
        Subscriber {
            inner: self.inner.clone(),
        }
    }

    pub fn notify_and_update(&self, block_height: BlockHeight) {
        let to_notify = {
            let mut inner_map = self.inner.write();

            // get all sending endpoint corresponding to subscribers that are waiting for a block height
            // that is at most `block_height`.
            let to_notify = inner_map.tx_handles.split_off(&Reverse(block_height));
            inner_map.latest_seen_block_height = block_height;

            to_notify.into_values().flatten()
        };

        for tx in to_notify {
            if let Err(e) = tx.send(()) {
                tracing::warn!("Failed to notify block height subscriber: {:?}", e);
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct Subscriber {
    inner: Arc<RwLock<HandlersMapInner>>,
}

impl Subscriber {
    pub async fn wait_for_block_height(
        &self,
        block_height: BlockHeight,
    ) -> anyhow::Result<()> {
        let future = {
            let mut inner_map = self.inner.write();

            if inner_map.latest_seen_block_height >= block_height {
                return Ok(());
            }

            let (tx, rx) = oneshot::channel();
            let handlers = inner_map
                .tx_handles
                .entry(Reverse(block_height))
                .or_default();
            handlers.push(tx);
            rx
        };

        future.await.map_err(|e| {
            anyhow::anyhow!("The block height subscription channel was closed: {:?}", e)
        })
    }

    pub fn latest_seen_block_height(&self) -> BlockHeight {
        self.inner.read().latest_seen_block_height
    }
}

#[derive(Debug, Default)]
struct HandlersMapInner {
    tx_handles: BTreeMap<Reverse<BlockHeight>, Vec<oneshot::Sender<()>>>,
    latest_seen_block_height: BlockHeight,
}

impl HandlersMapInner {
    fn new(latest_seen_block_height: BlockHeight) -> Self {
        Self {
            tx_handles: BTreeMap::new(),
            latest_seen_block_height,
        }
    }
}
