use crate::{
    db::{Error as DbStateError, KvStoreError},
    model::Coin,
    model::TxInfo,
};
use async_trait::async_trait;
use fuel_storage::Storage;
use fuel_tx::{ContractId, UtxoId};
use fuel_tx::{Transaction, TxId};
use fuel_vm::prelude::Contract;
use std::sync::Arc;
use thiserror::Error;

pub trait TxPoolDb:
    Storage<UtxoId, Coin, Error = KvStoreError>
    + Storage<ContractId, Contract, Error = DbStateError>
    + Send
    + Sync
{
    fn utxo(&self, utxo_id: &UtxoId) -> Result<Option<Coin>, KvStoreError> {
        Storage::<UtxoId, Coin>::get(self, utxo_id).map(|t| t.map(|t| t.as_ref().clone()))
    }

    fn contract_exist(&self, contract_id: ContractId) -> Result<bool, DbStateError> {
        Storage::<ContractId, Contract>::contains_key(self, &contract_id)
    }
}

/// Subscriber interface that receive inserted/removed events from txpool.
/// Usually needed for libp2p broadcasting and can be used to notify users of new tx.
#[async_trait]
pub trait Subscriber: Send + Sync {
    async fn inserted(&self, tx: Arc<Transaction>);

    async fn inserted_on_block_revert(&self, tx: Arc<Transaction>);

    async fn removed(&self, tx: Arc<Transaction>, error: &Error);
}

#[async_trait]
pub trait TxPool: Send + Sync {
    /// import list of transaction into txpool. All needed parents need to be known
    /// and parent->child order should be enforced in Vec, we will not do that check inside
    /// txpool and will just drop child and include only parent. Additional restrain is that
    /// child gas_price needs to be lower then parent gas_price. Transaction can be received
    /// from p2p **RespondTransactions** or from userland. Because of userland we are returning
    /// error for every insert for better user experience.
    async fn insert(&self, tx: Vec<Arc<Transaction>>)
        -> Vec<anyhow::Result<Vec<Arc<Transaction>>>>;

    /// find all tx by their hash
    async fn find(&self, hashes: &[TxId]) -> Vec<Option<TxInfo>>;

    /// find one tx by its hash
    async fn find_one(&self, hash: &TxId) -> Option<TxInfo>;

    /// find all dependent tx and return them with requsted dependencies in one list sorted by Price.
    async fn find_dependent(&self, hashes: &[TxId]) -> Vec<Arc<Transaction>>;

    /// Iterete over `hashes` and return all hashes that we dont have.
    /// Needed when we receive list of new hashed from peer with
    /// **BroadcastTransactionHashes**, so txpool needs to return
    /// tx that we dont have, and request them from that particular peer.
    async fn filter_by_negative(&self, hashes: &[TxId]) -> Vec<TxId>;

    /// Return all sorted transactions that are includable in next block.
    /// This is going to be heavy operation, use it with only when needed.
    async fn includable(&self) -> Vec<Arc<Transaction>>;

    /// When block is updated we need to receive all spend outputs and remove them from txpool
    /// There is three posibilities here. New Block is added or block is reverted/slashed from
    ///  chain or both. When new block is added we need to remove transactions from pool.
    /// Un case of block reverting we need to reinsert transaction in pool. What usually happens
    /// with reorg is that one block get reverted and one new gets add, in that sense it is highly
    /// probably that a lof of transactions that got reverted are going to be reinserted in new block.
    /// That explains why we have only one function and it contains two list of reverted/inserted
    /// transactions
    async fn block_update(&self /*spend_outputs: [Input], added_outputs: [AddedOutputs]*/);

    /// remove transaction from pool needed on user demand. Low priority
    async fn remove(&self, hashes: &[TxId]);

    async fn subscribe(&self, sub: Arc<dyn Subscriber>);
}

#[derive(Error, Debug, PartialEq, Eq, Clone)]
#[non_exhaustive]
pub enum Error {
    #[error("Transaction is not inserted. Hash is already known")]
    NotInsertedTxKnown,
    #[error("Transaction is not inserted. Pool limit is hit, try to increase gas_price")]
    NotInsertedLimitHit,
    #[error("TxPool required that transaction contains metadata")]
    NoMetadata,
    #[error("Transaction is not inserted. The gas price is too low.")]
    NotInsertedGasPriceTooLow,
    #[error("Transaction is not inserted. The byte price is too low.")]
    NotInsertedBytePriceTooLow,
    #[error(
        "Transaction is not inserted. More priced tx {0:#x} already spend this UTXO output: {1:#x}"
    )]
    NotInsertedCollision(TxId, UtxoId),
    #[error(
        "Transaction is not inserted. More priced tx has created contract with ContractId {0:#x}"
    )]
    NotInsertedCollisionContractId(ContractId),
    #[error("Transaction is not inserted. Dependent UTXO output is not existing: {0:#x}")]
    NotInsertedOutputNotExisting(UtxoId),
    #[error("Transaction is not inserted. UTXO input contract is not existing: {0:#x}")]
    NotInsertedInputContractNotExisting(ContractId),
    #[error("Transaction is not inserted. ContractId is already taken {0:#x}")]
    NotInsertedContractIdAlreadyTaken(ContractId),
    #[error("Transaction is not inserted. UTXO is not existing: {0:#x}")]
    NotInsertedInputUtxoIdNotExisting(UtxoId),
    #[error("Transaction is not inserted. UTXO is spent: {0:#x}")]
    NotInsertedInputUtxoIdSpent(UtxoId),
    #[error(
        "Transaction is not inserted. UTXO requires Contract input {0:#x} that is priced lower"
    )]
    NotInsertedContractPricedLower(ContractId),
    #[error("Transaction is not inserted. Input output mismatch. Coin owner is different from expected input")]
    NotInsertedIoWrongOwner,
    #[error("Transaction is not inserted. Input output mismatch. Coin output does not match expected input")]
    NotInsertedIoWrongAmount,
    #[error("Transaction is not inserted. Input output mismatch. Coin output asset_id does not match expected inputs")]
    NotInsertedIoWrongAssetId,
    #[error(
        "Transaction is not inserted. Input output mismatch. Expected coin but output is contract"
    )]
    NotInsertedIoConractOutput,
    #[error("Transaction is not inserted. Input output mismatch. Expected coin but output is withdrawal")]
    NotInsertedIoWithdrawalInput,
    #[error("Transaction is not inserted. Maximum depth of dependent transaction chain reached")]
    NotInsertedMaxDepth,
    // small todo for now it can pass but in future we should include better messages
    #[error("Transaction removed.")]
    Removed,
}
