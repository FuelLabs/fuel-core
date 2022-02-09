
use tokio::sync::oneshot;
use fuel_types::Bytes32;

pub enum SignerEvent {
    Sign {
        hash: Bytes32,
        response: oneshot::Sender<Result<Bytes32,anyhow::Error>>,
    }
}