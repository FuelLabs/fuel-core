use fuel_types::Bytes32;
use tokio::sync::oneshot;

pub enum SignerEvent {
    Sign {
        hash: Bytes32,
        response: oneshot::Sender<Result<Bytes32, anyhow::Error>>,
    },
}
