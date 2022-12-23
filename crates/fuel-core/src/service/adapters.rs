use crate::{
    database::Database,
    service::Config,
};
#[cfg(feature = "p2p")]
use fuel_core_p2p::orchestrator::Service as P2PService;
#[cfg(feature = "relayer")]
use fuel_core_relayer::RelayerSynced;
use fuel_core_txpool::Service as TxPoolService;
use fuel_core_types::blockchain::SealedBlock;
use std::sync::Arc;
use tokio::{
    sync::broadcast::Sender,
    task::JoinHandle,
};

pub mod poa;
pub mod producer;
pub mod txpool;

/// This is used to get block import events from coordinator source
/// and pass them to the txpool.
pub struct BlockImportAdapter {
    // TODO: We should use `fuel_core_poa::Service here but for that we need to fix
    //  the `start` of the process and store the task inside of the `Service`.
    tx: Sender<SealedBlock>,
}

pub struct TxPoolAdapter {
    service: TxPoolService,
}

impl TxPoolAdapter {
    pub fn new(service: TxPoolService) -> Self {
        Self { service }
    }
}

pub struct ExecutorAdapter {
    pub database: Database,
    pub config: Config,
}

pub struct MaybeRelayerAdapter {
    pub database: Database,
    #[cfg(feature = "relayer")]
    pub relayer_synced: Option<RelayerSynced>,
}

pub struct BlockProducerAdapter {
    pub block_producer: Arc<fuel_core_producer::Producer<Database>>,
}

#[cfg(feature = "p2p")]
#[derive(Clone)]
pub struct P2PAdapter {
    service: Arc<P2PService>,
}

#[cfg(not(feature = "p2p"))]
#[derive(Default, Clone)]
pub struct P2PAdapter;

#[cfg(feature = "p2p")]
impl P2PAdapter {
    pub fn new(service: Arc<P2PService>) -> Self {
        Self { service }
    }

    pub async fn stop(&self) -> Option<JoinHandle<()>> {
        self.service.stop().await
    }

    pub async fn start(&self) -> anyhow::Result<()> {
        self.service.start().await
    }
}

#[cfg(not(feature = "p2p"))]
impl P2PAdapter {
    pub fn new() -> Self {
        Default::default()
    }

    pub async fn stop(&self) -> Option<JoinHandle<()>> {
        None
    }

    pub async fn start(&self) -> anyhow::Result<()> {
        Ok(())
    }
}

// TODO: Create generic `Service` type that support `start` and `stop`.
