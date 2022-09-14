pub mod middleware;
pub use middleware::*;

use fuel_core_interfaces::{
    block_importer::ImportBlockBroadcast,
    relayer::RelayerRequest,
};
use tokio::sync::{
    broadcast,
    mpsc,
};

use crate::{
    mock_db::MockDb,
    service::Context,
    Config,
    Relayer,
};

pub async fn relayer(
    config: Config,
) -> (
    Relayer,
    mpsc::Sender<RelayerRequest>,
    broadcast::Sender<ImportBlockBroadcast>,
) {
    let db = Box::new(MockDb::default());
    let (request_sender, receiver) = mpsc::channel(10);
    let (broadcast_tx, new_block_event) = broadcast::channel(100);
    let private_key =
        hex::decode("c6bd905dcac2a0b1c43f574ab6933df14d7ceee0194902bce523ed054e8e798b")
            .unwrap();
    let ctx = Context {
        receiver,
        private_key,
        db,
        new_block_event,
        config,
    };

    let relayer = Relayer::new(ctx).await;
    (relayer, request_sender, broadcast_tx)
}
