use async_trait::async_trait;
use fuel_core_services::stream::BoxStream;
use fuel_core_storage::{
    iter::{
        BoxedIter,
        IterDirection,
    },
    tables::{
        BlobData,
        Coins,
        ContractsAssets,
        ContractsRawCode,
        Messages,
        StateTransitionBytecodeVersions,
        UploadedBytecodes,
    },
    Error as StorageError,
    Result as StorageResult,
    StorageInspect,
};
use fuel_core_txpool::service::TxStatusMessage;
use fuel_core_types::{
    blockchain::{
        block::CompressedBlock,
        consensus::Consensus,
        header::ConsensusParametersVersion,
        primitives::{
            BlockId,
            DaBlockHeight,
        },
    },
    entities::relayer::{
        message::{
            MerkleProof,
            Message,
        },
        transaction::RelayedTransactionStatus,
    },
    fuel_tx::{
        Bytes32,
        ConsensusParameters,
        Salt,
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
    fuel_vm::interpreter::Memory,
    services::{
        executor::TransactionExecutionStatus,
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

pub trait OffChainDatabase: Send + Sync {
    fn block_height(&self, block_id: &BlockId) -> StorageResult<BlockHeight>;

    fn tx_status(&self, tx_id: &TxId) -> StorageResult<TransactionStatus>;

    fn owned_coins_ids(
        &self,
        owner: &Address,
        start_coin: Option<UtxoId>,
        direction: IterDirection,
    ) -> BoxedIter<'_, StorageResult<UtxoId>>;

    fn owned_message_ids(
        &self,
        owner: &Address,
        start_message_id: Option<Nonce>,
        direction: IterDirection,
    ) -> BoxedIter<'_, StorageResult<Nonce>>;

    fn owned_transactions_ids(
        &self,
        owner: Address,
        start: Option<TxPointer>,
        direction: IterDirection,
    ) -> BoxedIter<StorageResult<(TxPointer, TxId)>>;

    fn contract_salt(&self, contract_id: &ContractId) -> StorageResult<Salt>;

    fn old_block(&self, height: &BlockHeight) -> StorageResult<CompressedBlock>;

    fn old_blocks(
        &self,
        height: Option<BlockHeight>,
        direction: IterDirection,
    ) -> BoxedIter<'_, StorageResult<CompressedBlock>>;

    fn old_block_consensus(&self, height: &BlockHeight) -> StorageResult<Consensus>;

    fn old_transaction(&self, id: &TxId) -> StorageResult<Option<Transaction>>;

    fn relayed_tx_status(
        &self,
        id: Bytes32,
    ) -> StorageResult<Option<RelayedTransactionStatus>>;

    fn message_is_spent(&self, nonce: &Nonce) -> StorageResult<bool>;
}

/// The on chain database port expected by GraphQL API service.
pub trait OnChainDatabase:
    Send
    + Sync
    + DatabaseBlocks
    + DatabaseMessages
    + StorageInspect<Coins, Error = StorageError>
    + StorageInspect<BlobData, Error = StorageError>
    + StorageInspect<StateTransitionBytecodeVersions, Error = StorageError>
    + StorageInspect<UploadedBytecodes, Error = StorageError>
    + DatabaseContracts
    + DatabaseChain
    + DatabaseMessageProof
{
}

/// Trait that specifies all the getters required for blocks.
pub trait DatabaseBlocks {
    /// Get a transaction by its id.
    fn transaction(&self, tx_id: &TxId) -> StorageResult<Transaction>;

    /// Get a block by its height.
    fn block(&self, height: &BlockHeight) -> StorageResult<CompressedBlock>;

    fn blocks(
        &self,
        height: Option<BlockHeight>,
        direction: IterDirection,
    ) -> BoxedIter<'_, StorageResult<CompressedBlock>>;

    fn latest_height(&self) -> StorageResult<BlockHeight>;

    /// Get the consensus for a block.
    fn consensus(&self, id: &BlockHeight) -> StorageResult<Consensus>;
}

/// Trait that specifies all the getters required for messages.
pub trait DatabaseMessages: StorageInspect<Messages, Error = StorageError> {
    fn all_messages(
        &self,
        start_message_id: Option<Nonce>,
        direction: IterDirection,
    ) -> BoxedIter<'_, StorageResult<Message>>;

    fn message_exists(&self, nonce: &Nonce) -> StorageResult<bool>;
}

pub trait DatabaseRelayedTransactions {
    fn transaction_status(
        &self,
        id: Bytes32,
    ) -> StorageResult<Option<RelayedTransactionStatus>>;
}

/// Trait that specifies all the getters required for contract.
pub trait DatabaseContracts:
    StorageInspect<ContractsRawCode, Error = StorageError>
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
    async fn dry_run_txs(
        &self,
        transactions: Vec<Transaction>,
        height: Option<BlockHeight>,
        utxo_validation: Option<bool>,
        gas_price: Option<u64>,
    ) -> anyhow::Result<Vec<TransactionExecutionStatus>>;
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

/// Trait for defining how to estimate gas price for future blocks
#[async_trait::async_trait]
pub trait GasPriceEstimate: Send + Sync {
    /// The worst case scenario for gas price at a given horizon
    async fn worst_case_gas_price(&self, height: BlockHeight) -> Option<u64>;
}

/// Trait for getting VM memory.
#[async_trait::async_trait]
pub trait MemoryPool {
    type Memory: Memory + Send + Sync + 'static;

    /// Get the memory instance.
    async fn get_memory(&self) -> Self::Memory;
}

pub mod worker {
    use super::super::storage::blocks::FuelBlockIdsToHeights;
    use crate::{
        fuel_core_graphql_api::storage::{
            coins::OwnedCoins,
            contracts::ContractsInfo,
            messages::{
                OwnedMessageIds,
                SpentMessages,
            },
        },
        graphql_api::storage::{
            old::{
                OldFuelBlockConsensus,
                OldFuelBlocks,
                OldTransactions,
            },
            relayed_transactions::RelayedTransactionStatuses,
        },
    };
    use fuel_core_services::stream::BoxStream;
    use fuel_core_storage::{
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

    pub trait OnChainDatabase: Send + Sync {
        /// Returns the latest block height.
        fn latest_height(&self) -> StorageResult<Option<BlockHeight>>;
    }

    pub trait OffChainDatabase: Send + Sync {
        type Transaction<'a>: OffChainDatabaseTransaction
        where
            Self: 'a;

        /// Returns the latest block height.
        fn latest_height(&self) -> StorageResult<Option<BlockHeight>>;

        /// Creates a write database transaction.
        fn transaction(&mut self) -> Self::Transaction<'_>;
    }

    pub trait OffChainDatabaseTransaction:
        StorageMutate<OwnedMessageIds, Error = StorageError>
        + StorageMutate<OwnedCoins, Error = StorageError>
        + StorageMutate<FuelBlockIdsToHeights, Error = StorageError>
        + StorageMutate<ContractsInfo, Error = StorageError>
        + StorageMutate<OldFuelBlocks, Error = StorageError>
        + StorageMutate<OldFuelBlockConsensus, Error = StorageError>
        + StorageMutate<OldTransactions, Error = StorageError>
        + StorageMutate<SpentMessages, Error = StorageError>
        + StorageMutate<RelayedTransactionStatuses, Error = StorageError>
    {
        fn record_tx_id_owner(
            &mut self,
            owner: &Address,
            block_height: BlockHeight,
            tx_idx: u16,
            tx_id: &Bytes32,
        ) -> StorageResult<()>;

        fn update_tx_status(
            &mut self,
            id: &Bytes32,
            status: TransactionStatus,
        ) -> StorageResult<Option<TransactionStatus>>;

        /// Update metadata about the total number of transactions on the chain.
        /// Returns the total count after the update.
        fn increase_tx_count(&mut self, new_txs_count: u64) -> StorageResult<u64>;

        /// Gets the total number of transactions on the chain from metadata.
        fn get_tx_count(&self) -> StorageResult<u64>;

        /// Commits the underlying changes into the database.
        fn commit(self) -> StorageResult<()>;
    }

    pub trait BlockImporter: Send + Sync {
        /// Returns a stream of imported block.
        fn block_events(&self) -> BoxStream<SharedImportResult>;

        /// Return the import result at the given height.
        fn block_event_at_height(
            &self,
            height: Option<BlockHeight>,
        ) -> anyhow::Result<SharedImportResult>;
    }

    pub trait TxPool: Send + Sync {
        /// Sends the complete status of the transaction.
        fn send_complete(
            &self,
            id: Bytes32,
            block_height: &BlockHeight,
            status: TransactionStatus,
        );
    }
}

pub trait ConsensusProvider: Send + Sync {
    /// Returns latest consensus parameters.
    fn latest_consensus_params(&self) -> Arc<ConsensusParameters>;

    fn consensus_params_at_version(
        &self,
        version: &ConsensusParametersVersion,
    ) -> anyhow::Result<Arc<ConsensusParameters>>;
}
