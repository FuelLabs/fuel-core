use crate::service::adapters::{
    BlockImportAdapter,
    P2PAdapter,
};
use async_trait::async_trait;
use fuel_core_txpool::ports::BlockImport;
use fuel_core_types::{
    blockchain::SealedBlock,
    fuel_tx::Transaction,
    services::p2p::{
        GossipsubMessageAcceptance,
        TransactionGossipData,
    },
};
use std::sync::Arc;
use tokio::sync::broadcast::Receiver;

impl BlockImportAdapter {
    pub fn new(rx: Receiver<SealedBlock>) -> Self {
        Self { rx }
    }
}

#[async_trait]
impl BlockImport for BlockImportAdapter {
    async fn next_block(&mut self) -> SealedBlock {
        match self.rx.recv().await {
            Ok(block) => return block,
            Err(err) => {
                panic!("Block import channel errored unexpectedly: {err:?}");
            }
        }
    }
}

#[cfg(feature = "p2p")]
#[async_trait]
impl fuel_core_txpool::ports::PeerToPeer for P2PAdapter {
    type GossipedTransaction = TransactionGossipData;

    async fn broadcast_transaction(
        &self,
        transaction: Arc<Transaction>,
    ) -> anyhow::Result<()> {
        self.p2p_service.broadcast_transaction(transaction).await
    }

    async fn next_gossiped_transaction(&mut self) -> Self::GossipedTransaction {
        // todo: handle unwrap
        self.tx_receiver.recv().await.unwrap()
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
#[async_trait]
impl fuel_core_txpool::ports::PeerToPeer for P2PAdapter {
    type GossipedTransaction = TransactionGossipData;

    async fn broadcast_transaction(
        &self,
        _transaction: Arc<Transaction>,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    async fn next_gossiped_transaction(&mut self) -> Self::GossipedTransaction {
        core::future::pending::<Self::GossipedTransaction>().await
    }

    async fn notify_gossip_transaction_validity(
        &self,
        _message: &Self::GossipedTransaction,
        _validity: GossipsubMessageAcceptance,
    ) {
        // no-op
    }
}
