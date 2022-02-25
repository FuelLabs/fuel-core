pub mod middleware;
pub use middleware::*;

use fuel_core_interfaces::{
    block_importer::NewBlockEvent, db::helpers::DummyDB, relayer::RelayerEvent,
    signer::helpers::DummySigner,
};
use tokio::sync::{broadcast, mpsc, Mutex};

use crate::{Config, Relayer};

pub fn relayer(
    config: Config,
) -> (
    Relayer,
    mpsc::Sender<RelayerEvent>,
    broadcast::Sender<NewBlockEvent>,
) {
    let db = Box::new(Mutex::new(DummyDB::filled()));
    let (tx, rx) = mpsc::channel(10);
    let (broadcast_tx, broadcast_rx) = broadcast::channel(10);
    let signer = Box::new(DummySigner {});
    let relayer = Relayer::new(config, db, rx, broadcast_rx, signer);
    (relayer, tx, broadcast_tx)
}

pub fn provider() -> Option<()> {
    None
}
