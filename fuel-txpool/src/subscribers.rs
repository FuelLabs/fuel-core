use std::sync::Arc;

use crate::{types::ArcTx, Error};
use async_trait::async_trait;
use interfaces::txpool::Subscriber;
use parking_lot::RwLock;

pub struct MultiSubscriber {
    annons: RwLock<Vec<Arc<dyn Subscriber>>>,
}

impl MultiSubscriber {
    pub fn new() -> Self {
        Self {
            annons: RwLock::new(Vec::new()),
        }
    }

    pub fn sub(&self, sub: Arc<dyn Subscriber>) {
        self.annons.write().push(sub);
    }
}

#[async_trait]
impl Subscriber for MultiSubscriber {
    async fn inserted(&self, tx: ArcTx) {
        let subs = self.annons.read().clone();
        for sub in subs.iter() {
            let tx = tx.clone();
            sub.inserted(tx).await;
        }
    }

    async fn inserted_on_block_revert(&self, tx: ArcTx) {
        let subs = self.annons.read().clone();
        for sub in subs.iter() {
            //let annon = **annon;
            let tx = tx.clone();
            sub.inserted_on_block_revert(tx.clone()).await;
        }
    }

    async fn removed(&self, tx: ArcTx, error: &Error) {
        let subs = self.annons.read().clone();
        for sub in subs.iter() {
            sub.removed(tx.clone(), error).await;
        }
    }
}
