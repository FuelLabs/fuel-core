use crate::{
    database::Database,
    service::Config,
};
use fuel_core_types::blockchain::SealedBlock;
use std::sync::Arc;
use tokio::sync::broadcast::Sender;

pub mod poa;
pub mod producer;
pub mod txpool;

/// This is used to get block import events from coordinator source
/// and pass them to the txpool.
#[derive(Clone)]
pub struct BlockImportAdapter {
    // TODO: We should use `fuel_core_poa::Service here but for that we need to fix
    //  the `start` of the process and store the task inside of the `Service`.
    pub tx: Sender<SealedBlock>,
}

pub struct TxPoolAdapter {
    shared_state: fuel_core_txpool::service::SharedState<P2PAdapter>,
}

impl TxPoolAdapter {
    pub fn new(shared_state: fuel_core_txpool::service::SharedState<P2PAdapter>) -> Self {
        Self { shared_state }
    }
}

pub struct ExecutorAdapter {
    pub database: Database,
    pub config: Config,
}

pub struct MaybeRelayerAdapter {
    pub database: Database,
    #[cfg(feature = "relayer")]
    pub relayer_synced: Option<fuel_core_relayer::RelayerSynced>,
}

pub struct BlockProducerAdapter {
    pub block_producer: Arc<fuel_core_producer::Producer<Database>>,
}

#[cfg(feature = "p2p")]
#[derive(Clone)]
pub struct P2PAdapter {
    shared_state: fuel_core_p2p::service::SharedState,
}

#[cfg(not(feature = "p2p"))]
#[derive(Default, Clone)]
pub struct P2PAdapter;

#[cfg(feature = "p2p")]
impl P2PAdapter {
    pub fn new(shared_state: fuel_core_p2p::service::SharedState) -> Self {
        Self { shared_state }
    }
}

#[cfg(not(feature = "p2p"))]
impl P2PAdapter {
    pub fn new() -> Self {
        Default::default()
    }
}
