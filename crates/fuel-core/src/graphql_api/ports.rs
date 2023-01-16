use crate::{
    database::Database,
    state::IterDirection,
    txpool::service::TxUpdate,
};
use anyhow::Result;
use async_trait::async_trait;
use fuel_core_storage::{
    iter::BoxedIter,
    tables::{
        Coins,
        ContractsAssets,
        ContractsInfo,
        ContractsRawCode,
        FuelBlocks,
        Messages,
        Receipts,
        SealedBlockConsensus,
        Transactions,
    },
    transactional::StorageTransaction,
    Error as StorageError,
    Result as StorageResult,
    StorageInspect,
};
use fuel_core_types::{
    blockchain::primitives::{
        BlockHeight,
        BlockId,
        DaBlockHeight,
    },
    entities::message::Message,
    fuel_tx::{
        Receipt,
        Transaction,
        TxId,
        TxPointer,
        UtxoId,
    },
    fuel_types::{
        Address,
        AssetId,
        ContractId,
        MessageId,
    },
    services::{
        executor::{
            ExecutionBlock,
            Result as ExecutorResult,
            UncommittedResult,
        },
        graphql_api::ContractBalance,
        txpool::{
            InsertionResult,
            TransactionStatus,
            TxInfo,
        },
    },
};
use std::sync::Arc;

/// The database port expected by GraphQL API service.
pub trait DatabasePort:
    Send
    + Sync
    + DatabaseBlocks
    + DatabaseTransactions
    + DatabaseMessages
    + DatabaseCoins
    + DatabaseContracts
    + DatabaseChain
{
}

/// Trait that specifies all the getters required for blocks.
pub trait DatabaseBlocks:
    StorageInspect<FuelBlocks, Error = StorageError>
    + StorageInspect<SealedBlockConsensus, Error = StorageError>
{
    fn block_id(&self, height: BlockHeight) -> StorageResult<BlockId>;

    fn blocks_ids(
        &self,
        start: Option<BlockHeight>,
        direction: IterDirection,
    ) -> BoxedIter<'_, StorageResult<(BlockHeight, BlockId)>>;

    fn ids_of_latest_block(&self) -> StorageResult<(BlockHeight, BlockId)>;
}

/// Trait that specifies all the getters required for transactions.
pub trait DatabaseTransactions:
    StorageInspect<Transactions, Error = StorageError>
    + StorageInspect<Receipts, Error = StorageError>
{
    fn tx_status(&self, tx_id: &TxId) -> StorageResult<TransactionStatus>;

    fn owned_transactions_ids(
        &self,
        owner: &Address,
        start: Option<TxPointer>,
        direction: IterDirection,
    ) -> BoxedIter<StorageResult<(TxPointer, TxId)>>;
}

/// Trait that specifies all the getters required for messages.
pub trait DatabaseMessages: StorageInspect<Messages, Error = StorageError> {
    fn owned_message_ids(
        &self,
        owner: &Address,
        start_message_id: Option<MessageId>,
        direction: IterDirection,
    ) -> BoxedIter<'_, StorageResult<MessageId>>;

    fn all_messages(
        &self,
        start_message_id: Option<MessageId>,
        direction: IterDirection,
    ) -> BoxedIter<'_, StorageResult<Message>>;
}

/// Trait that specifies all the getters required for coins.
pub trait DatabaseCoins: StorageInspect<Coins, Error = StorageError> {
    fn owned_coins_ids(
        &self,
        owner: &Address,
        start_coin: Option<UtxoId>,
        direction: IterDirection,
    ) -> BoxedIter<'_, StorageResult<UtxoId>>;
}

/// Trait that specifies all the getters required for contract.
pub trait DatabaseContracts:
    StorageInspect<ContractsRawCode, Error = StorageError>
    + StorageInspect<ContractsInfo, Error = StorageError>
    + for<'a> StorageInspect<ContractsAssets<'a>, Error = StorageError>
{
    fn contract_balances(
        &self,
        contract: ContractId,
        start_asset: Option<AssetId>,
        direction: IterDirection,
    ) -> BoxedIter<StorageResult<ContractBalance>>;
}

/// Trait that specifies all the getters required for chain metadata.
pub trait DatabaseChain {
    fn chain_name(&self) -> StorageResult<String>;

    fn base_chain_height(&self) -> StorageResult<DaBlockHeight>;
}

pub trait FindTx {
    fn find_one(&self, id: TxId) -> Option<TxInfo>;
}

pub trait InsertTx {
    fn insert(&self, txs: Vec<Arc<Transaction>>) -> Vec<anyhow::Result<InsertionResult>>;
}

pub trait TxSubscription {
    fn tx_update_subscribe(&self) -> tokio::sync::broadcast::Receiver<TxUpdate>;
}

pub trait TxPoolPort: Send + Sync + FindTx + InsertTx + TxSubscription {}

#[async_trait]
pub trait DryRunExecution {
    async fn dry_run_tx(
        &self,
        transaction: Transaction,
        height: Option<BlockHeight>,
        utxo_validation: Option<bool>,
    ) -> Result<Vec<Receipt>>;
}

pub trait BlockProducerPort: Send + Sync + DryRunExecution {}

pub trait ExecuteWithoutCommit {
    fn execute_with_no_commit(
        &self,
        block: ExecutionBlock,
    ) -> ExecutorResult<UncommittedResult<StorageTransaction<Database>>>;
}

pub trait ExecutorPort: Send + Sync + ExecuteWithoutCommit {}
