use crate::{
    db::{Error as DbStateError, KvStoreError},
    model::Coin,
    model::TxInfo,
};
use fuel_storage::Storage;
use fuel_tx::{ContractId, UtxoId};
use fuel_tx::{Transaction, TxId};
use fuel_vm::prelude::Contract;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::oneshot;

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

#[derive(Debug)]
pub enum TxPoolMpsc {
    /// Return all sorted transactions that are includable in next block.
    /// This is going to be heavy operation, use it only when needed.
    Includable {
        response: oneshot::Sender<Vec<Arc<Transaction>>>,
    },
    /// import list of transaction into txpool. All needed parents need to be known
    /// and parent->child order should be enforced in Vec, we will not do that check inside
    /// txpool and will just drop child and include only parent. Additional restrain is that
    /// child gas_price needs to be lower then parent gas_price. Transaction can be received
    /// from p2p **RespondTransactions** or from userland. Because of userland we are returning
    /// error for every insert for better user experience.
    Insert {
        txs: Vec<Arc<Transaction>>,
        response: oneshot::Sender<Vec<anyhow::Result<Vec<Arc<Transaction>>>>>,
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
    /// find all dependent tx and return them with requsted dependencies in one list sorted by Price.
    FindDependent {
        ids: Vec<TxId>,
        response: oneshot::Sender<Vec<Arc<Transaction>>>,
    },
    /// remove transaction from pool needed on user demand. Low priority
    Remove { ids: Vec<TxId> },
    /// Iterete over `hashes` and return all hashes that we dont have.
    /// Needed when we receive list of new hashed from peer with
    /// **BroadcastTransactionHashes**, so txpool needs to return
    /// tx that we dont have, and request them from that particular peer.
    FilterByNegative {
        ids: Vec<TxId>,
        response: oneshot::Sender<Vec<TxId>>,
    },
    /// stop txpool
    Stop,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TxStatus {
    /// Submitted into txpool.
    Submitted,
    /// Executed in fuel block.
    Executed,
    /// removed from txpool.
    SqueezedOut { reason: Error },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TxStatusBroadcast {
    pub tx: Arc<Transaction>,
    pub status: TxStatus,
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
