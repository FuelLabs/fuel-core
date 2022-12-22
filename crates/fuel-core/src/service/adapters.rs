use crate::{
    database::Database,
    service::Config,
};
#[cfg(feature = "p2p")]
use fuel_core_p2p::orchestrator::Service as P2PService;
#[cfg(feature = "relayer")]
use fuel_core_relayer::RelayerSynced;
use fuel_core_txpool::Service as TxPoolService;
use fuel_core_types::{
    blockchain::SealedBlock,
    services::txpool::TxStatus,
};
use std::sync::Arc;
use tokio::{
    sync::broadcast::Receiver,
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
    rx: Receiver<SealedBlock>,
}

pub struct TxPoolAdapter {
    service: TxPoolService,
    tx_status_rx: Option<Receiver<TxStatus>>,
}

impl TxPoolAdapter {
    pub fn new(service: TxPoolService) -> Self {
        Self {
            service,
            tx_status_rx: None,
        }
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
pub struct P2PAdapter {
    p2p_service: Arc<P2PService>,
    tx_receiver: Option<Receiver<fuel_core_types::services::p2p::TransactionGossipData>>,
}

#[cfg(not(feature = "p2p"))]
#[derive(Default, Clone)]
pub struct P2PAdapter;

#[cfg(feature = "p2p")]
impl Clone for P2PAdapter {
    fn clone(&self) -> Self {
        Self::new(self.p2p_service.clone())
    }
}

#[cfg(feature = "p2p")]
impl P2PAdapter {
    pub fn new(p2p_service: Arc<P2PService>) -> Self {
        Self {
            p2p_service,
            // don't autogenerate a fresh receiver unless it is actually used
            // otherwise we may encounter "lagged" errors on recv
            // https://docs.rs/tokio/latest/tokio/sync/broadcast/index.html#lagging
            tx_receiver: None,
        }
    }

    pub async fn stop(&self) -> Option<JoinHandle<()>> {
        self.p2p_service.stop().await
    }

    pub async fn start(&self) -> anyhow::Result<()> {
        self.p2p_service.start().await
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
