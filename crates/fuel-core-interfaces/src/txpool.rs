use derive_more::{
    Deref,
    DerefMut,
};
use fuel_core_storage::{
    tables::{
        Coins,
        ContractsRawCode,
        Messages,
    },
    Error as StorageError,
    Result as StorageResult,
    StorageAsRef,
    StorageInspect,
};
use fuel_core_types::{
    blockchain::primitives::BlockHeight,
    entities::{
        coin::Coin,
        message::Message,
    },
    fuel_tx::{
        Transaction,
        TxId,
        UtxoId,
    },
    fuel_types::{
        ContractId,
        MessageId,
    },
    services::txpool::{
        ArcPoolTx,
        InsertionResult,
        TxInfo,
    },
};
use std::{
    fmt::Debug,
    sync::Arc,
};
use tokio::sync::{
    mpsc,
    oneshot,
};

pub trait TxPoolDb:
    StorageInspect<Coins, Error = StorageError>
    + StorageInspect<ContractsRawCode, Error = StorageError>
    + StorageInspect<Messages, Error = StorageError>
    + Send
    + Sync
{
    fn utxo(&self, utxo_id: &UtxoId) -> StorageResult<Option<Coin>> {
        self.storage::<Coins>()
            .get(utxo_id)
            .map(|t| t.map(|t| t.as_ref().clone()))
    }

    fn contract_exist(&self, contract_id: &ContractId) -> StorageResult<bool> {
        self.storage::<ContractsRawCode>().contains_key(contract_id)
    }

    fn message(&self, message_id: &MessageId) -> StorageResult<Option<Message>> {
        self.storage::<Messages>()
            .get(message_id)
            .map(|t| t.map(|t| t.as_ref().clone()))
    }

    fn current_block_height(&self) -> StorageResult<BlockHeight>;
}

/// RPC client for doing calls to the TxPool through an MPSC channel.
#[derive(Clone, Deref, DerefMut)]
pub struct Sender(mpsc::Sender<TxPoolMpsc>);

impl Sender {
    pub fn new(sender: mpsc::Sender<TxPoolMpsc>) -> Self {
        Self(sender)
    }

    pub async fn insert(
        &self,
        txs: Vec<Arc<Transaction>>,
    ) -> anyhow::Result<Vec<anyhow::Result<InsertionResult>>> {
        let (response, receiver) = oneshot::channel();
        self.send(TxPoolMpsc::Insert { txs, response }).await?;
        receiver.await.map_err(Into::into)
    }

    pub async fn find(&self, ids: Vec<TxId>) -> anyhow::Result<Vec<Option<TxInfo>>> {
        let (response, receiver) = oneshot::channel();
        self.send(TxPoolMpsc::Find { ids, response }).await?;
        receiver.await.map_err(Into::into)
    }

    pub async fn find_one(&self, id: TxId) -> anyhow::Result<Option<TxInfo>> {
        let (response, receiver) = oneshot::channel();
        self.send(TxPoolMpsc::FindOne { id, response }).await?;
        receiver.await.map_err(Into::into)
    }

    pub async fn find_dependent(&self, ids: Vec<TxId>) -> anyhow::Result<Vec<ArcPoolTx>> {
        let (response, receiver) = oneshot::channel();
        self.send(TxPoolMpsc::FindDependent { ids, response })
            .await?;
        receiver.await.map_err(Into::into)
    }

    pub async fn filter_by_negative(&self, ids: Vec<TxId>) -> anyhow::Result<Vec<TxId>> {
        let (response, receiver) = oneshot::channel();
        self.send(TxPoolMpsc::FilterByNegative { ids, response })
            .await?;
        receiver.await.map_err(Into::into)
    }

    pub async fn includable(&self) -> anyhow::Result<Vec<ArcPoolTx>> {
        let (response, receiver) = oneshot::channel();
        self.send(TxPoolMpsc::Includable { response }).await?;
        receiver.await.map_err(Into::into)
    }

    pub async fn remove(&self, ids: Vec<TxId>) -> anyhow::Result<Vec<ArcPoolTx>> {
        let (response, receiver) = oneshot::channel();
        self.send(TxPoolMpsc::Remove { ids, response }).await?;
        receiver.await.map_err(Into::into)
    }

    pub fn channel(buffer: usize) -> (Sender, mpsc::Receiver<TxPoolMpsc>) {
        let (sender, reciever) = mpsc::channel(buffer);
        (Sender(sender), reciever)
    }
}

#[async_trait::async_trait]
impl super::poa_coordinator::TransactionPool for Sender {
    async fn pending_number(&self) -> anyhow::Result<usize> {
        let (response, receiver) = oneshot::channel();
        self.send(TxPoolMpsc::PendingNumber { response }).await?;
        receiver.await.map_err(Into::into)
    }

    async fn total_consumable_gas(&self) -> anyhow::Result<u64> {
        let (response, receiver) = oneshot::channel();
        self.send(TxPoolMpsc::ConsumableGas { response }).await?;
        receiver.await.map_err(Into::into)
    }

    async fn remove_txs(&mut self, ids: Vec<TxId>) -> anyhow::Result<Vec<ArcPoolTx>> {
        let (response, receiver) = oneshot::channel();
        self.send(TxPoolMpsc::Remove { ids, response }).await?;
        receiver.await.map_err(Into::into)
    }
}

/// RPC commands that can be sent to the TxPool through an MPSC channel.
/// Responses are returned using `response` oneshot channel.
#[derive(Debug)]
pub enum TxPoolMpsc {
    /// The number of pending transactions in the pool.
    PendingNumber { response: oneshot::Sender<usize> },
    /// The amount of gas in all includable transactions combined
    ConsumableGas { response: oneshot::Sender<u64> },
    /// Return all sorted transactions that are includable in next block.
    /// This is going to be heavy operation, use it only when needed.
    Includable {
        response: oneshot::Sender<Vec<ArcPoolTx>>,
    },
    /// import list of transaction into txpool. All needed parents need to be known
    /// and parent->child order should be enforced in Vec, we will not do that check inside
    /// txpool and will just drop child and include only parent. Additional restrain is that
    /// child gas_price needs to be lower then parent gas_price. Transaction can be received
    /// from p2p **RespondTransactions** or from userland. Because of userland we are returning
    /// error for every insert for better user experience.
    Insert {
        txs: Vec<Arc<Transaction>>,
        response: oneshot::Sender<Vec<anyhow::Result<InsertionResult>>>,
    },
    /// find all tx by their hash
    Find {
        ids: Vec<TxId>,
        response: oneshot::Sender<Vec<Option<TxInfo>>>,
    },
    /// find one tx by its hash
    FindOne {
        id: TxId,
        response: oneshot::Sender<Option<TxInfo>>,
    },
    /// find all dependent tx and return them with requested dependencies in one list sorted by Price.
    FindDependent {
        ids: Vec<TxId>,
        response: oneshot::Sender<Vec<ArcPoolTx>>,
    },
    /// remove transaction from pool needed on user demand. Low priority
    Remove {
        ids: Vec<TxId>,
        response: oneshot::Sender<Vec<ArcPoolTx>>,
    },
    /// Iterate over `hashes` and return all hashes that we don't have.
    /// Needed when we receive list of new hashed from peer with
    /// **BroadcastTransactionHashes**, so txpool needs to return
    /// tx that we don't have, and request them from that particular peer.
    FilterByNegative {
        ids: Vec<TxId>,
        response: oneshot::Sender<Vec<TxId>>,
    },
    /// stop txpool
    Stop,
}
