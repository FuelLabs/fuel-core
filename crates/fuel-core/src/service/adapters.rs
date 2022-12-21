use crate::{
    database::Database,
    service::Config,
};
#[cfg(feature = "p2p")]
use fuel_core_p2p::orchestrator::Service as P2PService;
#[cfg(feature = "relayer")]
use fuel_core_relayer::RelayerSynced;
use fuel_core_txpool::Service;
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
    pub service: Arc<Service>,
    pub tx_status_rx: Receiver<TxStatus>,
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

pub struct PoACoordinatorAdapter {
    pub block_producer: Arc<fuel_core_producer::Producer<Database>>,
}

#[cfg_attr(not(feature = "p2p"), derive(Clone))]
pub struct P2PAdapter {
    #[cfg(feature = "p2p")]
    p2p_service: Arc<P2PService>,
    #[cfg(feature = "p2p")]
    tx_receiver: Receiver<fuel_core_types::services::p2p::TransactionGossipData>,
}

#[cfg(feature = "p2p")]
impl Clone for P2PAdapter {
    fn clone(&self) -> Self {
        Self::new(self.p2p_service.clone())
    }
}

#[cfg(feature = "p2p")]
impl P2PAdapter {
    pub fn new(p2p_service: Arc<P2PService>) -> Self {
        let tx_receiver = p2p_service.subscribe_tx();
        Self {
            p2p_service,
            tx_receiver,
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
        Self {}
    }

    pub async fn stop(&self) -> Option<JoinHandle<()>> {
        None
    }

    pub async fn start(&self) -> anyhow::Result<()> {
        Ok(())
    }
}

// TODO: Create generic `Service` type that support `start` and `stop`.
