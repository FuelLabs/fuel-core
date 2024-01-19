use async_trait::async_trait;
use fuel_core_services::stream::BoxStream;
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
        Transactions,
    },
    Error as StorageError,
    Result as StorageResult,
    StorageInspect,
};
use fuel_core_txpool::service::TxStatusMessage;
use fuel_core_types::{
    blockchain::{
        block::CompressedBlock,
        primitives::{
            BlockId,
            DaBlockHeight,
        },
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
        p2p::PeerInfo,
        txpool::{
            InsertionResult,
            TransactionStatus,
        },
    },
    tai64::Tai64,
};
use std::sync::Arc;

pub trait OffChainDatabase:
    Send + Sync + StorageInspect<Receipts, Error = StorageError>
{
    fn owned_message_ids(
        &self,
        owner: &Address,
        start_message_id: Option<Nonce>,
        direction: IterDirection,
    ) -> BoxedIter<'_, StorageResult<Nonce>>;

    fn owned_coins_ids(
        &self,
        owner: &Address,
        start_coin: Option<UtxoId>,
        direction: IterDirection,
    ) -> BoxedIter<'_, StorageResult<UtxoId>>;

    fn tx_status(&self, tx_id: &TxId) -> StorageResult<TransactionStatus>;

    fn owned_transactions_ids(
        &self,
        owner: Address,
        start: Option<TxPointer>,
        direction: IterDirection,
    ) -> BoxedIter<StorageResult<(TxPointer, TxId)>>;
}

/// The on chain database port expected by GraphQL API service.
pub trait OnChainDatabase:
    Send
    + Sync
    + DatabaseBlocks
    + StorageInspect<Transactions, Error = StorageError>
    + DatabaseMessages
    + StorageInspect<Coins, Error = StorageError>
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
    fn block_height(&self, block_id: &BlockId) -> StorageResult<BlockHeight>;

    fn blocks(
        &self,
        height: Option<BlockHeight>,
        direction: IterDirection,
    ) -> BoxedIter<'_, StorageResult<CompressedBlock>>;

    fn latest_height(&self) -> StorageResult<BlockHeight>;
}

/// Trait that specifies all the getters required for messages.
pub trait DatabaseMessages: StorageInspect<Messages, Error = StorageError> {
    fn all_messages(
        &self,
        start_message_id: Option<Nonce>,
        direction: IterDirection,
    ) -> BoxedIter<'_, StorageResult<Message>>;

    fn message_is_spent(&self, nonce: &Nonce) -> StorageResult<bool>;

    fn message_exists(&self, nonce: &Nonce) -> StorageResult<bool>;
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

    fn da_height(&self) -> StorageResult<DaBlockHeight>;
}

#[async_trait]
pub trait TxPoolPort: Send + Sync {
    fn transaction(&self, id: TxId) -> Option<Transaction>;

    fn submission_time(&self, id: TxId) -> Option<Tai64>;

    async fn insert(
        &self,
        txs: Vec<Arc<Transaction>>,
    ) -> Vec<anyhow::Result<InsertionResult>>;

    fn tx_update_subscribe(
        &self,
        tx_id: TxId,
    ) -> anyhow::Result<BoxStream<TxStatusMessage>>;
}

#[async_trait]
pub trait BlockProducerPort: Send + Sync {
    async fn dry_run_tx(
        &self,
        transaction: Transaction,
        height: Option<BlockHeight>,
        utxo_validation: Option<bool>,
    ) -> anyhow::Result<Vec<Receipt>>;
}

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

#[async_trait::async_trait]
pub trait P2pPort: Send + Sync {
    async fn all_peer_info(&self) -> anyhow::Result<Vec<PeerInfo>>;
}

pub mod worker {
    use fuel_core_services::stream::BoxStream;
    use fuel_core_storage::{
        tables::Receipts,
        transactional::Transactional,
        Error as StorageError,
        Result as StorageResult,
        StorageMutate,
    };
    use fuel_core_types::{
        fuel_tx::{
            Address,
            Bytes32,
        },
        fuel_types::BlockHeight,
        services::{
            block_importer::SharedImportResult,
            txpool::TransactionStatus,
        },
    };

    pub trait OffChainDatabase:
        Send
        + Sync
        + StorageMutate<Receipts, Error = StorageError>
        + Transactional<Storage = Self>
    {
        fn record_tx_id_owner(
            &mut self,
            owner: &Address,
            block_height: BlockHeight,
            tx_idx: u16,
            tx_id: &Bytes32,
        ) -> StorageResult<Option<Bytes32>>;

        fn update_tx_status(
            &mut self,
            id: &Bytes32,
            status: TransactionStatus,
        ) -> StorageResult<Option<TransactionStatus>>;
    }

    pub trait BlockImporter {
        /// Returns a stream of imported block.
        fn block_events(&self) -> BoxStream<SharedImportResult>;
    }
}
