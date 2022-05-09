pub mod middleware;
pub use middleware::*;

use fuel_core_interfaces::{
    block_importer::NewBlockEvent, db::helpers::DummyDb, relayer::RelayerEvent,
    signer::helpers::DummySigner,
};
use tokio::sync::{broadcast, mpsc};

use crate::{Config, Relayer};

pub fn relayer(
    config: Config,
) -> (
    Relayer,
    mpsc::Sender<RelayerEvent>,
    broadcast::Sender<NewBlockEvent>,
) {
    let db = Box::new(DummyDb::filled());
    let (relayer_event_tx, relayer_event_rx) = mpsc::channel(10);
    let (broadcast_tx, broadcast_rx) = broadcast::channel(100);
    let signer = Box::new(DummySigner {});
    let relayer = Relayer::new(config, db, relayer_event_rx, broadcast_rx, signer);
    (relayer, relayer_event_tx, broadcast_tx)
}

pub fn provider() -> Option<()> {
    None
}
