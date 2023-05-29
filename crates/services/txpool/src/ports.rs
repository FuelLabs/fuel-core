use fuel_core_services::stream::BoxStream;
use fuel_core_storage::Result as StorageResult;
use fuel_core_types::{
    entities::{
        coins::coin::CompressedCoin,
        message::Message,
    },
    fuel_tx::{
        Transaction,
        UtxoId,
    },
    fuel_types::{
        BlockHeight,
        Bytes32,
        ContractId,
        Nonce,
    },
    services::{
        block_importer::ImportResult,
        p2p::{
            GossipsubMessageAcceptance,
            GossipsubMessageInfo,
            NetworkData,
        },
        txpool::TransactionStatus,
    },
};
use std::sync::Arc;

pub trait PeerToPeer: Send + Sync {
    type GossipedTransaction: NetworkData<Transaction>;

    // Gossip broadcast a transaction inserted via API.
    fn broadcast_transaction(&self, transaction: Arc<Transaction>) -> anyhow::Result<()>;

    /// Creates a stream of next transactions gossiped from the network.
    fn gossiped_transaction_events(&self) -> BoxStream<Self::GossipedTransaction>;

    // Report the validity of a transaction received from the network.
    fn notify_gossip_transaction_validity(
        &self,
        message_info: GossipsubMessageInfo,
        validity: GossipsubMessageAcceptance,
    ) -> anyhow::Result<()>;
}

pub trait BlockImporter: Send + Sync {
    /// Wait until the next block is available
    fn block_events(&self) -> BoxStream<Arc<ImportResult>>;
}

pub trait TxPoolDb: Send + Sync {
    fn utxo(&self, utxo_id: &UtxoId) -> StorageResult<Option<CompressedCoin>>;

    fn contract_exist(&self, contract_id: &ContractId) -> StorageResult<bool>;

    fn message(&self, message_id: &Nonce) -> StorageResult<Option<Message>>;

    fn is_message_spent(&self, message_id: &Nonce) -> StorageResult<bool>;

    fn current_block_height(&self) -> StorageResult<BlockHeight>;

    fn transaction_status(&self, tx_id: &Bytes32) -> StorageResult<TransactionStatus>;
}
