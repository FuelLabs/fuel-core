use crate::{
    database::Database,
    service::Config,
};
#[cfg(feature = "p2p")]
use fuel_core_p2p::orchestrator::Service as P2pService;
#[cfg(feature = "relayer")]
use fuel_core_relayer::RelayerSynced;
use fuel_core_txpool::Service;
use fuel_core_types::{
    blockchain::SealedBlock,
    fuel_tx::Transaction,
    services::{
        p2p::{
            GossipsubMessageAcceptance,
            TransactionGossipData,
        },
        txpool::TxStatus,
    },
};
use std::sync::Arc;
use tokio::sync::broadcast::Receiver;
#[cfg(not(feature = "p2p"))]
use tokio::sync::oneshot;
#[cfg(feature = "p2p")]
use tokio::task::JoinHandle;

pub mod poa;
pub mod producer;
pub mod txpool;

/// This is used to get block import events from coordinator source
/// and pass them to the txpool.
pub struct BlockImportAdapter {
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

pub struct P2pAdapter {
    #[cfg(feature = "p2p")]
    p2p_service: P2pService,
}

#[cfg(feature = "p2p")]
impl P2pAdapter {
    pub fn new(p2p_service: P2pService) -> Self {
        Self { p2p_service }
    }

    pub async fn stop(&self) -> Option<JoinHandle<()>> {
        self.p2p_service.stop().await
    }

    pub async fn start(&self) -> anyhow::Result<()> {
        self.p2p_service.start().await
    }
}

#[cfg(feature = "p2p")]
#[async_trait::async_trait]
impl fuel_core_txpool::ports::PeerToPeer for P2pAdapter {
    type GossipedTransaction = TransactionGossipData;

    async fn broadcast_transaction(
        &self,
        transaction: Arc<Transaction>,
    ) -> anyhow::Result<()> {
        self.p2p_service.broadcast_transaction(transaction).await
    }

    async fn next_gossiped_transaction(&self) -> Self::GossipedTransaction {
        // todo: handle unwrap
        self.p2p_service.next_gossiped_transaction().await.unwrap()
    }

    async fn notify_gossip_transaction_validity(
        &self,
        message: &Self::GossipedTransaction,
        validity: GossipsubMessageAcceptance,
    ) {
        self.p2p_service
            .notify_gossip_transaction_validity(message, validity)
            .await;
    }
}

#[cfg(not(feature = "p2p"))]
#[async_trait::async_trait]
impl fuel_core_txpool::ports::PeerToPeer for P2pAdapter {
    type GossipedTransaction = TransactionGossipData;

    async fn broadcast_transaction(
        &self,
        _transaction: Arc<Transaction>,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    async fn next_gossiped_transaction(&self) -> Self::GossipedTransaction {
        // hold the await and never return
        let (_tx, rx) = oneshot::channel::<()>();
        let _ = rx.await;
        // the await should never yield
        unreachable!();
    }

    async fn notify_gossip_transaction_validity(
        &self,
        _message: &Self::GossipedTransaction,
        _validity: GossipsubMessageAcceptance,
    ) {
        // no-op
    }
}
