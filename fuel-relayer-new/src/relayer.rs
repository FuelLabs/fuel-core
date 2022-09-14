use std::sync::Arc;

use tokio::sync::Notify;

type Synced = Arc<Notify>;

pub struct Relayer {
    synced: Synced,
}

impl Relayer {
    pub fn new() -> Self {
        let synced = Arc::new(Notify::new());
        let r = Self {
            synced: synced.clone(),
        };
        run(synced);
        r
    }

    pub async fn await_synced(&self) {
        self.synced.notified().await;
    }
}

fn run(synced: Synced) {
    let jh = tokio::task::spawn(async move {
        loop {
            synced.notify_waiters();
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    });
}
