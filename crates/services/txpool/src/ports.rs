use fuel_core_storage::Result as StorageResult;
use fuel_core_types::{
    blockchain::{
        primitives::BlockHeight,
        SealedBlock,
    },
    entities::{
        coin::Coin,
        message::Message,
    },
    fuel_tx::{
        Transaction,
        UtxoId,
    },
    fuel_types::{
        ContractId,
        MessageId,
    },
    services::p2p::{
        GossipsubMessageAcceptance,
        NetworkData,
    },
};
use std::sync::Arc;

#[async_trait::async_trait]
pub trait PeerToPeer: Send + Sync {
    type GossipedTransaction: NetworkData<Transaction>;

    // Gossip broadcast a transaction inserted via API.
    async fn broadcast_transaction(
        &self,
        transaction: Arc<Transaction>,
    ) -> anyhow::Result<()>;

    // Await the next transaction from network gossip (similar to stream.next()).
    async fn next_gossiped_transaction(&mut self) -> Self::GossipedTransaction;

    // Report the validity of a transaction received from the network.
    async fn notify_gossip_transaction_validity(
        &self,
        message: &Self::GossipedTransaction,
        validity: GossipsubMessageAcceptance,
    );
}

#[async_trait::async_trait]
pub trait BlockImport: Send + Sync {
    /// Wait until the next block is available
    async fn next_block(&mut self) -> SealedBlock;
}

pub trait TxPoolDb: Send + Sync {
    fn utxo(&self, utxo_id: &UtxoId) -> StorageResult<Option<Coin>>;

    fn contract_exist(&self, contract_id: &ContractId) -> StorageResult<bool>;

    fn message(&self, message_id: &MessageId) -> StorageResult<Option<Message>>;

    fn current_block_height(&self) -> StorageResult<BlockHeight>;
}
