use async_trait::async_trait;
use fuel_core_services::stream::{
    BoxFuture,
    BoxStream,
};
use fuel_core_storage::{
    iter::{
        BoxedIter,
        IterDirection,
    },
    tables::{
        Coins,
        ContractsAssets,
        ContractsInfo,
        ContractsRawCode,
        FuelBlocks,
        Messages,
        Receipts,
        SealedBlockConsensus,
        SpentMessages,
        Transactions,
    },
    Error as StorageError,
    Result as StorageResult,
    StorageInspect,
};
use fuel_core_txpool::service::TxStatusMessage;
use fuel_core_types::{
    blockchain::primitives::{
        BlockId,
        DaBlockHeight,
    },
    entities::message::{
        MerkleProof,
        Message,
    },
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
        BlockHeight,
        ContractId,
        Nonce,
    },
    services::{
        graphql_api::ContractBalance,
        txpool::{
            InsertionResult,
            TransactionStatus,
        },
    },
    tai64::Tai64,
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
    + DatabaseMessageProof
{
}

/// Trait that specifies all the getters required for blocks.
pub trait DatabaseBlocks:
    StorageInspect<FuelBlocks, Error = StorageError>
    + StorageInspect<SealedBlockConsensus, Error = StorageError>
{
    fn block_id(&self, height: &BlockHeight) -> StorageResult<BlockId>;

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
        owner: Address,
        start: Option<TxPointer>,
        direction: IterDirection,
    ) -> BoxedIter<StorageResult<(TxPointer, TxId)>>;
}

/// Trait that specifies all the getters required for messages.
pub trait DatabaseMessages:
    StorageInspect<Messages, Error = StorageError>
    + StorageInspect<SpentMessages, Error = StorageError>
{
    fn owned_message_ids(
        &self,
        owner: &Address,
        start_message_id: Option<Nonce>,
        direction: IterDirection,
    ) -> BoxedIter<'_, StorageResult<Nonce>>;

    fn all_messages(
        &self,
        start_message_id: Option<Nonce>,
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
    + StorageInspect<ContractsAssets, Error = StorageError>
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

pub trait TxPoolPort: Send + Sync {
    fn transaction(&self, id: TxId) -> Option<Transaction>;

    fn submission_time(&self, id: TxId) -> Option<Tai64>;

    fn insert(&self, txs: Vec<Arc<Transaction>>) -> Vec<anyhow::Result<InsertionResult>>;

    fn tx_update_subscribe(
        &self,
        tx_id: TxId,
    ) -> BoxFuture<'_, BoxStream<TxStatusMessage>>;
}

#[async_trait]
pub trait DryRunExecution {
    async fn dry_run_tx(
        &self,
        transaction: Transaction,
        height: Option<BlockHeight>,
        utxo_validation: Option<bool>,
    ) -> anyhow::Result<Vec<Receipt>>;
}

pub trait BlockProducerPort: Send + Sync + DryRunExecution {}

#[async_trait::async_trait]
pub trait ConsensusModulePort: Send + Sync {
    async fn manually_produce_blocks(
        &self,
        start_time: Option<Tai64>,
        number_of_blocks: u32,
    ) -> anyhow::Result<()>;
}

/// Trait that specifies queries supported by the database.
pub trait DatabaseMessageProof: Send + Sync {
    /// Gets the [`MerkleProof`] for the message block at `message_block_height` height
    /// relatively to the commit block where message block <= commit block.
    fn block_history_proof(
        &self,
        message_block_height: &BlockHeight,
        commit_block_height: &BlockHeight,
    ) -> StorageResult<MerkleProof>;
}
