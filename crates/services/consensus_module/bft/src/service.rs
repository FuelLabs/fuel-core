use crate::Config;
use parking_lot::Mutex;
use tokio::{
    sync::mpsc,
    task::JoinHandle,
};

pub struct Service {
    join: Mutex<Option<JoinHandle<()>>>,
    sender: mpsc::Sender<()>,
}

impl Service {
    pub async fn new(_config: &Config, _db: ()) -> anyhow::Result<Self> {
        let (sender, _receiver) = mpsc::channel(100);
        Ok(Self {
            sender,
            join: Mutex::new(None),
        })
    }

    pub async fn start(&self) {
        let mut join = self.join.lock();
        if join.is_none() {
            *join = Some(tokio::spawn(async {}));
        }
    }

    pub async fn stop(&self) -> Option<JoinHandle<()>> {
        let join = self.join.lock().take();
        if join.is_some() {
            let _ = self.sender.send(()).await;
        }
        join
    }

    pub fn sender(&self) -> &mpsc::Sender<()> {
        &self.sender
    }
}
