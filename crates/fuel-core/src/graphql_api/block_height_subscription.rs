use std::{
    collections::BTreeMap,
    sync::Arc,
};

use fuel_core_types::fuel_types::BlockHeight;
use parking_lot::RwLock;
use std::future::IntoFuture;
use tokio::sync::oneshot;

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
struct Reverse<T>(T);

impl<T> PartialOrd<Reverse<T>> for Reverse<T>
where
    T: PartialOrd,
{
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        other.0.partial_cmp(&self.0)
    }
}

impl<T> Ord for Reverse<T>
where
    T: Ord,
{
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        other.0.cmp(&self.0)
    }
}

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
                tracing::error!("Failed to notify block height subscriber: {:?}", e);
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct Subscriber {
    inner: Arc<RwLock<HandlersMapInner>>,
}

impl Subscriber {
    pub fn wait_for_block_height(&self, block_height: BlockHeight) -> Handle {
        let (tx, rx) = oneshot::channel();
        let mut inner_map = self.inner.write();
        let handlers = inner_map
            .tx_handles
            .entry(Reverse(block_height))
            .or_default();
        handlers.push(tx);
        Handle { rx }
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

pub struct Handle {
    rx: oneshot::Receiver<()>,
}

impl IntoFuture for Handle {
    // TODO: Change error type and do not leak receiver abstraction
    type Output = Result<(), oneshot::error::RecvError>;

    type IntoFuture = oneshot::Receiver<()>;

    fn into_future(self) -> Self::IntoFuture {
        self.rx
    }
}
