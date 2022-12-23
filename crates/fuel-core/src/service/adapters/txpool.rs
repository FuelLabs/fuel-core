use crate::service::adapters::{
    BlockImportAdapter,
    P2PAdapter,
};
use fuel_core_services::stream::BoxStream;
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
use tokio::sync::broadcast::Sender;

impl BlockImportAdapter {
    pub fn new(tx: Sender<SealedBlock>) -> Self {
        Self { tx }
    }
}

impl BlockImport for BlockImportAdapter {
    fn block_events(&self) -> BoxStream<SealedBlock> {
        use tokio_stream::{
            wrappers::BroadcastStream,
            StreamExt,
        };
        Box::pin(
            BroadcastStream::new(self.tx.subscribe()).filter_map(|result| result.ok()),
        )
    }
}

#[cfg(feature = "p2p")]
impl fuel_core_txpool::ports::PeerToPeer for P2PAdapter {
    type GossipedTransaction = TransactionGossipData;

    fn broadcast_transaction(&self, transaction: Arc<Transaction>) -> anyhow::Result<()> {
        self.service.broadcast_transaction(transaction)
    }

    fn gossiped_transaction_events(&self) -> BoxStream<Self::GossipedTransaction> {
        use tokio_stream::{
            wrappers::BroadcastStream,
            StreamExt,
        };
        Box::pin(
            BroadcastStream::new(self.service.subscribe_tx())
                .filter_map(|result| result.ok()),
        )
    }

    fn notify_gossip_transaction_validity(
        &self,
        message: &Self::GossipedTransaction,
        validity: GossipsubMessageAcceptance,
    ) -> anyhow::Result<()> {
        self.service
            .notify_gossip_transaction_validity(message, validity)
    }
}

#[cfg(not(feature = "p2p"))]
impl fuel_core_txpool::ports::PeerToPeer for P2PAdapter {
    type GossipedTransaction = TransactionGossipData;

    fn broadcast_transaction(
        &self,
        _transaction: Arc<Transaction>,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    fn gossiped_transaction_events(&self) -> BoxStream<Self::GossipedTransaction> {
        Box::pin(fuel_core_services::stream::pending())
    }

    fn notify_gossip_transaction_validity(
        &self,
        _message: &Self::GossipedTransaction,
        _validity: GossipsubMessageAcceptance,
    ) -> anyhow::Result<()> {
        Ok(())
    }
}
