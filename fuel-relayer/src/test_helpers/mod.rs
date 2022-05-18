pub mod middleware;
pub use middleware::*;

use fuel_core_interfaces::{
    block_importer::NewBlockEvent, db::helpers::DummyDb, relayer::RelayerEvent,
};
use tokio::sync::{broadcast, mpsc};

use crate::{Config, Relayer};

pub async fn relayer(
    config: Config,
) -> (
    Relayer,
    mpsc::Sender<RelayerEvent>,
    broadcast::Sender<NewBlockEvent>,
) {
    let db = Box::new(DummyDb::filled());
    let (relayer_event_tx, relayer_event_rx) = mpsc::channel(10);
    let (broadcast_tx, broadcast_rx) = broadcast::channel(100);
    let private_key =
        hex::decode("c6bd905dcac2a0b1c43f574ab6933df14d7ceee0194902bce523ed054e8e798b").unwrap();

    let relayer = Relayer::new(config, &private_key, db, relayer_event_rx, broadcast_rx).await;
    (relayer, relayer_event_tx, broadcast_tx)
}

pub fn provider() -> Option<()> {
    None
}
