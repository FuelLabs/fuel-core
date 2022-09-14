use anyhow::Result;
use std::ops::Deref;
use tokio::sync::watch;

type Synced = watch::Receiver<bool>;

pub struct RelayerHandle {
    synced: Synced,
}

impl RelayerHandle {
    pub fn start() -> Self {
        let (tx, rx) = watch::channel(false);
        let synced = rx;
        let r = Self {
            synced: synced.clone(),
        };
        run(Relayer { synced: tx });
        r
    }

    pub async fn await_synced(&self) -> Result<()> {
        let mut rx = self.synced.clone();
        if !rx.borrow_and_update().deref() {
            rx.changed().await?;
        }
        Ok(())
    }
}

pub struct Relayer {
    synced: watch::Sender<bool>,
}

fn run(relayer: Relayer) {
    let jh = tokio::task::spawn(async move {
        loop {
            if relayer.synced.send(true).is_err() {
                break
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    });
}
