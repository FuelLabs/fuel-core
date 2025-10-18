use crate::{
    database::database_description::IndexationKind,
    fuel_core_graphql_api::{
        database::arc_wrapper::ArcWrapper,
        ports::{
            OffChainDatabase,
            OffChainDatabaseAt,
            OnChainDatabase,
            OnChainDatabaseAt,
        },
    },
};
use fuel_core_services::yield_stream::StreamYieldExt;
use fuel_core_storage::{
    Error as StorageError,
    IsNotFound,
    Mappable,
    PredicateStorageRequirements,
    Result as StorageResult,
    StorageInspect,
    StorageRead,
    StorageSize,
    iter::{
        BoxedIter,
        IntoBoxedIter,
        IterDirection,
    },
    not_found,
    tables::Transactions,
    transactional::HistoricalView,
};
use fuel_core_types::{
    blockchain::{
        block::CompressedBlock,
        consensus::Consensus,
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
        Address,
        AssetId,
        Bytes32,
        ContractId,
        Salt,
        Transaction,
        TxId,
        TxPointer,
        UtxoId,
    },
    fuel_types::{
        BlobId,
        BlockHeight,
        Nonce,
    },
    fuel_vm::BlobData,
    services::{
        graphql_api::ContractBalance,
        transaction_status::TransactionExecutionStatus,
    },
};
use futures::Stream;
use std::{
    borrow::Cow,
    sync::Arc,
};
use strum::IntoEnumIterator;

use super::ports::worker;

mod arc_wrapper;

/// The on-chain view of the database used by the [`ReadView`] to fetch on-chain data.
pub type OnChainView = Arc<dyn OnChainDatabase>;
/// The off-chain view of the database used by the [`ReadView`] to fetch off-chain data.
pub type OffChainView = Arc<dyn OffChainDatabase>;

/// The on-chain view of the database used by the [`ReadViewAt`] to fetch on-chain data.
pub type OnChainViewAt = Arc<dyn OnChainDatabaseAt>;
/// The off-chain view of the database used by the [`ReadViewAt`] to fetch off-chain data.
pub type OffChainViewAt = Arc<dyn OffChainDatabaseAt>;

/// The container of the on-chain and off-chain database view provides.
/// It is used only by `ViewExtension` to create a [`ReadView`].
pub struct ReadDatabase {
    /// The size of the batch during fetching from the database.
    batch_size: usize,
    /// The height of the genesis block.
    genesis_height: BlockHeight,
    /// The on-chain database view provider.
    on_chain: Box<
        dyn HistoricalView<
                LatestView = OnChainView,
                Height = BlockHeight,
                ViewAtHeight = OnChainViewAt,
            >,
    >,
    /// The off-chain database view provider.
    off_chain: Box<
        dyn HistoricalView<
                LatestView = OffChainView,
                Height = BlockHeight,
                ViewAtHeight = OffChainViewAt,
            >,
    >,
    /// The flag indicating which indexation is enabled.
    indexation_flags: IndexationFlags,
}

#[derive(Clone)]
pub struct IndexationFlags(u8);

impl IndexationFlags {
    pub fn new() -> Self {
        Self(0)
    }

    pub fn contains(&self, kind: &IndexationKind) -> bool {
        self.0 & (1 << *kind as u8) != 0
    }

    pub fn insert(&mut self, kind: IndexationKind) {
        self.0 |= 1 << kind as u8;
    }
}

impl Default for IndexationFlags {
    fn default() -> Self {
        Self::new()
    }
}

impl ReadDatabase {
    /// Creates a new [`ReadDatabase`] with the given on-chain and off-chain database view providers.
    pub fn new<OnChain, OffChain>(
        batch_size: usize,
        genesis_height: BlockHeight,
        on_chain: OnChain,
        off_chain: OffChain,
    ) -> Result<Self, StorageError>
    where
        OnChain: HistoricalView<Height = BlockHeight> + 'static,
        OffChain:
            HistoricalView<Height = BlockHeight> + worker::OffChainDatabase + 'static,
        OnChain::LatestView: OnChainDatabase,
        OffChain::LatestView: OffChainDatabase,
        OnChain::ViewAtHeight: OnChainDatabaseAt,
        OffChain::ViewAtHeight: OffChainDatabaseAt,
    {
        let mut indexation_flags = IndexationFlags::new();
        for kind in IndexationKind::iter() {
            match kind {
                IndexationKind::Balances => {
                    if off_chain.balances_indexation_enabled()? {
                        indexation_flags.insert(kind);
                    }
                }
                IndexationKind::CoinsToSpend => {
                    if off_chain.coins_to_spend_indexation_enabled()? {
                        indexation_flags.insert(kind);
                    }
                }
                IndexationKind::AssetMetadata => {
                    if off_chain.asset_metadata_indexation_enabled()? {
                        indexation_flags.insert(kind);
                    }
                }
            }
        }
        Ok(Self {
            batch_size,
            genesis_height,
            on_chain: Box::new(ArcWrapper::new(on_chain)),
            off_chain: Box::new(ArcWrapper::new(off_chain)),
            indexation_flags,
        })
    }

    /// Creates a consistent view of the database.
    pub fn view(&self) -> StorageResult<ReadView> {
        // TODO: Use the same height for both views to guarantee consistency.
        //  It is not possible to implement until `view_at` is implemented for the `AtomicView`.
        //  https://github.com/FuelLabs/fuel-core/issues/1582
        Ok(ReadView {
            batch_size: self.batch_size,
            genesis_height: self.genesis_height,
            on_chain: self.on_chain.latest_view()?,
            off_chain: self.off_chain.latest_view()?,
            indexation_flags: self.indexation_flags.clone(),
        })
    }

    /// Creates a consistent view of the database at specific block height.
    pub fn view_at(&self, block_height: BlockHeight) -> StorageResult<ReadViewAt> {
        Ok(ReadViewAt {
            batch_size: self.batch_size,
            genesis_height: self.genesis_height,
            view_block_height: block_height,
            on_chain: self.on_chain.view_at(&block_height)?,
            off_chain: self.off_chain.view_at(&block_height)?,
            indexation_flags: self.indexation_flags.clone(),
        })
    }

    #[cfg(feature = "test-helpers")]
    pub fn test_view(&self) -> ReadView {
        self.view().expect("The latest view always should exist")
    }
}

#[derive(Clone)]
pub struct ReadView {
    pub(crate) batch_size: usize,
    pub(crate) genesis_height: BlockHeight,
    pub(crate) on_chain: OnChainView,
    pub(crate) off_chain: OffChainView,
    pub(crate) indexation_flags: IndexationFlags,
}

impl ReadView {
    pub fn transaction(&self, tx_id: &TxId) -> StorageResult<Transaction> {
        let result = self.on_chain.transaction(tx_id);
        if result.is_not_found() {
            match self.off_chain.old_transaction(tx_id)? {
                Some(tx) => Ok(tx),
                _ => Err(not_found!(Transactions)),
            }
        } else {
            result
        }
    }

    pub async fn transactions(
        &self,
        tx_ids: Vec<TxId>,
    ) -> Vec<StorageResult<Transaction>> {
        // TODO: Use multiget when it's implemented.
        //  https://github.com/FuelLabs/fuel-core/issues/2344
        let result = tx_ids
            .iter()
            .map(|tx_id| self.transaction(tx_id))
            .collect::<Vec<_>>();
        // Give a chance to other tasks to run.
        tokio::task::yield_now().await;
        result
    }

    pub fn block(&self, height: &BlockHeight) -> StorageResult<CompressedBlock> {
        if *height >= self.genesis_height {
            self.on_chain.block(height)
        } else {
            self.off_chain.old_block(height)
        }
    }

    pub fn blocks(
        &self,
        height: Option<BlockHeight>,
        direction: IterDirection,
    ) -> BoxedIter<'_, StorageResult<CompressedBlock>> {
        // Chain together blocks from the off-chain db and the on-chain db
        // The blocks in off-chain db, if any, are from time before regenesis

        if let Some(height) = height {
            match (height >= self.genesis_height, direction) {
                (true, IterDirection::Forward) => {
                    self.on_chain.blocks(Some(height), direction)
                }
                (true, IterDirection::Reverse) => self
                    .on_chain
                    .blocks(Some(height), direction)
                    .chain(self.off_chain.old_blocks(None, direction))
                    .into_boxed(),
                (false, IterDirection::Forward) => self
                    .off_chain
                    .old_blocks(Some(height), direction)
                    .chain(self.on_chain.blocks(None, direction))
                    .into_boxed(),
                (false, IterDirection::Reverse) => {
                    self.off_chain.old_blocks(Some(height), direction)
                }
            }
        } else {
            match direction {
                IterDirection::Forward => self
                    .off_chain
                    .old_blocks(None, direction)
                    .chain(self.on_chain.blocks(None, direction))
                    .into_boxed(),
                IterDirection::Reverse => self
                    .on_chain
                    .blocks(None, direction)
                    .chain(self.off_chain.old_blocks(None, direction))
                    .into_boxed(),
            }
        }
    }

    pub fn latest_height(&self) -> StorageResult<BlockHeight> {
        self.on_chain.latest_height()
    }

    pub fn consensus(&self, id: &BlockHeight) -> StorageResult<Consensus> {
        if *id >= self.genesis_height {
            self.on_chain.consensus(id)
        } else {
            self.off_chain.old_block_consensus(id)
        }
    }
}

impl StorageInspect<BlobData> for ReadView {
    type Error = StorageError;

    fn get(
        &self,
        key: &<BlobData as Mappable>::Key,
    ) -> StorageResult<Option<Cow<'_, <BlobData as Mappable>::OwnedValue>>> {
        StorageInspect::<BlobData>::get(self.on_chain.as_ref(), key)
    }

    fn contains_key(&self, key: &<BlobData as Mappable>::Key) -> StorageResult<bool> {
        StorageInspect::<BlobData>::contains_key(self.on_chain.as_ref(), key)
    }
}

impl StorageSize<BlobData> for ReadView {
    fn size_of_value(&self, key: &BlobId) -> Result<Option<usize>, Self::Error> {
        StorageSize::<BlobData>::size_of_value(self.on_chain.as_ref(), key)
    }
}

impl StorageRead<BlobData> for ReadView {
    fn read(
        &self,
        key: &BlobId,
        offset: usize,
        buf: &mut [u8],
    ) -> Result<bool, Self::Error> {
        StorageRead::<BlobData>::read(self.on_chain.as_ref(), key, offset, buf)
    }

    fn read_alloc(&self, key: &BlobId) -> Result<Option<Vec<u8>>, Self::Error> {
        StorageRead::<BlobData>::read_alloc(self.on_chain.as_ref(), key)
    }
}

impl PredicateStorageRequirements for ReadView {
    fn storage_error_to_string(error: Self::Error) -> String {
        error.to_string()
    }
}

impl ReadView {
    pub fn all_messages(
        &self,
        start_message_id: Option<Nonce>,
        direction: IterDirection,
    ) -> impl Stream<Item = StorageResult<Message>> + '_ {
        futures::stream::iter(self.on_chain.all_messages(start_message_id, direction))
            .yield_each(self.batch_size)
    }

    pub fn message_exists(&self, nonce: &Nonce) -> StorageResult<bool> {
        self.on_chain.message_exists(nonce)
    }

    pub fn relayed_transaction_status(
        &self,
        id: Bytes32,
    ) -> StorageResult<Option<RelayedTransactionStatus>> {
        self.off_chain.relayed_tx_status(id)
    }

    pub fn contract_balances(
        &self,
        contract: ContractId,
        start_asset: Option<AssetId>,
        direction: IterDirection,
    ) -> impl Stream<Item = StorageResult<ContractBalance>> + '_ {
        futures::stream::iter(self.on_chain.contract_balances(
            contract,
            start_asset,
            direction,
        ))
        .yield_each(self.batch_size)
    }

    pub fn contract_storage_slots(
        &self,
        contract: ContractId,
    ) -> impl Stream<Item = StorageResult<(Bytes32, Vec<u8>)>> + '_ {
        futures::stream::iter(self.on_chain.contract_storage_slots(contract))
            .yield_each(self.batch_size)
    }

    pub fn contract_storage_balances(
        &self,
        contract: ContractId,
    ) -> impl Stream<Item = StorageResult<ContractBalance>> + '_ {
        futures::stream::iter(self.on_chain.contract_storage_balances(contract))
            .yield_each(self.batch_size)
    }

    pub fn da_height(&self) -> StorageResult<DaBlockHeight> {
        self.on_chain.da_height()
    }

    pub fn block_history_proof(
        &self,
        message_block_height: &BlockHeight,
        commit_block_height: &BlockHeight,
    ) -> StorageResult<MerkleProof> {
        self.on_chain
            .block_history_proof(message_block_height, commit_block_height)
    }
}

impl ReadView {
    pub fn block_height(&self, block_id: &BlockId) -> StorageResult<BlockHeight> {
        self.off_chain.block_height(block_id)
    }

    pub fn tx_status(&self, tx_id: &TxId) -> StorageResult<TransactionExecutionStatus> {
        self.off_chain.tx_status(tx_id)
    }

    pub fn owned_coins_ids(
        &self,
        owner: &Address,
        start_coin: Option<UtxoId>,
        direction: IterDirection,
    ) -> impl Stream<Item = StorageResult<UtxoId>> + '_ + use<'_> {
        let iter = self.off_chain.owned_coins_ids(owner, start_coin, direction);

        futures::stream::iter(iter)
    }

    pub fn owned_message_ids(
        &self,
        owner: &Address,
        start_message_id: Option<Nonce>,
        direction: IterDirection,
    ) -> impl Stream<Item = StorageResult<Nonce>> + '_ + use<'_> {
        futures::stream::iter(self.off_chain.owned_message_ids(
            owner,
            start_message_id,
            direction,
        ))
    }

    pub fn owned_transactions_ids(
        &self,
        owner: Address,
        start: Option<TxPointer>,
        direction: IterDirection,
    ) -> impl Stream<Item = StorageResult<(TxPointer, TxId)>> + '_ {
        futures::stream::iter(
            self.off_chain
                .owned_transactions_ids(owner, start, direction),
        )
    }

    pub fn contract_salt(&self, contract_id: &ContractId) -> StorageResult<Salt> {
        self.off_chain.contract_salt(contract_id)
    }

    pub fn relayed_tx_status(
        &self,
        id: Bytes32,
    ) -> StorageResult<Option<RelayedTransactionStatus>> {
        self.off_chain.relayed_tx_status(id)
    }

    pub fn message_is_spent(&self, nonce: &Nonce) -> StorageResult<bool> {
        self.off_chain.message_is_spent(nonce)
    }
}

#[derive(Clone)]
#[allow(dead_code)]
pub struct ReadViewAt {
    pub(crate) batch_size: usize,
    pub(crate) genesis_height: BlockHeight,
    pub(crate) view_block_height: BlockHeight,
    pub(crate) on_chain: OnChainViewAt,
    pub(crate) off_chain: OffChainViewAt,
    pub(crate) indexation_flags: IndexationFlags,
}

impl ReadViewAt {
    pub fn contract_slot_values(
        &self,
        contract_id: ContractId,
        storage_slots: Vec<Bytes32>,
    ) -> impl Stream<Item = StorageResult<(Bytes32, Vec<u8>)>> + '_ {
        futures::stream::iter(
            self.on_chain
                .contract_slot_values(contract_id, storage_slots),
        )
        .yield_each(self.batch_size)
    }

    pub fn contract_balance_values(
        &self,
        contract_id: ContractId,
        assets: Vec<AssetId>,
    ) -> impl Stream<Item = StorageResult<ContractBalance>> + '_ {
        futures::stream::iter(self.on_chain.contract_balance_values(contract_id, assets))
            .yield_each(self.batch_size)
    }
}

#[test]
fn test_indexation_flags() {
    let mut indexation = IndexationFlags::new();
    assert!(!indexation.contains(&IndexationKind::Balances));
    assert!(!indexation.contains(&IndexationKind::CoinsToSpend));
    assert!(!indexation.contains(&IndexationKind::AssetMetadata));

    indexation.insert(IndexationKind::Balances);
    assert!(indexation.contains(&IndexationKind::Balances));
    assert!(!indexation.contains(&IndexationKind::CoinsToSpend));
    assert!(!indexation.contains(&IndexationKind::AssetMetadata));

    indexation.insert(IndexationKind::CoinsToSpend);
    assert!(indexation.contains(&IndexationKind::Balances));
    assert!(indexation.contains(&IndexationKind::CoinsToSpend));
    assert!(!indexation.contains(&IndexationKind::AssetMetadata));

    indexation.insert(IndexationKind::AssetMetadata);
    assert!(indexation.contains(&IndexationKind::Balances));
    assert!(indexation.contains(&IndexationKind::CoinsToSpend));
    assert!(indexation.contains(&IndexationKind::AssetMetadata));
}
