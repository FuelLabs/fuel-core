use crate::{
    database::{
        database_description::off_chain::OffChain,
        Database,
    },
    fuel_core_graphql_api::{
        ports::{
            worker::Transactional,
            OffChainDatabase,
        },
        storage::{
            contracts::ContractsInfo,
            old::OldFuelBlockMerkleMetadata,
            relayed_transactions::RelayedTransactionStatuses,
            transactions::OwnedTransactionIndexCursor,
        },
    },
    graphql_api::{
        ports::{
            DatabaseBlocks,
            DatabaseMerkle,
        },
        storage::old::{
            OldFuelBlockConsensus,
            OldFuelBlockMerkleData,
            OldFuelBlocks,
            OldTransactions,
        },
    },
};
use fuel_core_storage::{
    iter::{
        BoxedIter,
        IntoBoxedIter,
        IterDirection,
        IteratorOverTable,
    },
    not_found,
    tables::merkle::{
        DenseMerkleMetadata,
        DenseMetadataKey,
    },
    transactional::{
        IntoTransaction,
        StorageTransaction,
    },
    Error as StorageError,
    Result as StorageResult,
    StorageAsRef,
};
use fuel_core_txpool::types::{
    ContractId,
    TxId,
};
use fuel_core_types::{
    blockchain::{
        block::CompressedBlock,
        consensus::Consensus,
        primitives::BlockId,
    },
    entities::relayer::transaction::RelayedTransactionStatus,
    fuel_merkle::binary,
    fuel_tx::{
        Address,
        Bytes32,
        Salt,
        Transaction,
        TxPointer,
        UtxoId,
    },
    fuel_types::{
        BlockHeight,
        Nonce,
    },
    services::txpool::TransactionStatus,
};

use std::borrow::Cow;

impl DatabaseBlocks for Database<OffChain> {
    fn transaction(&self, tx_id: &TxId) -> StorageResult<Transaction> {
        self.storage_as_ref::<OldTransactions>()
            .get(tx_id)?
            .map(Cow::into_owned)
            .ok_or(not_found!(OldTransactions))
    }

    fn block(&self, height: &BlockHeight) -> StorageResult<CompressedBlock> {
        self.storage_as_ref::<OldFuelBlocks>()
            .get(height)?
            .map(Cow::into_owned)
            .ok_or(not_found!(OldFuelBlocks))
    }

    fn blocks(
        &self,
        height: Option<BlockHeight>,
        direction: IterDirection,
    ) -> BoxedIter<'_, StorageResult<CompressedBlock>> {
        self.iter_all_by_start::<OldFuelBlocks>(height.as_ref(), Some(direction))
            .map(|r| r.map(|(_, block)| block))
            .into_boxed()
    }

    fn latest_height(&self) -> StorageResult<BlockHeight> {
        self.iter_all::<OldFuelBlocks>(Some(IterDirection::Reverse))
            .next()
            .transpose()?
            .map(|(height, _)| height)
            .ok_or(not_found!("BlockHeight"))
    }

    fn consensus(&self, height: &BlockHeight) -> StorageResult<Consensus> {
        self.storage_as_ref::<OldFuelBlockConsensus>()
            .get(height)?
            .map(Cow::into_owned)
            .ok_or(not_found!(OldFuelBlockConsensus))
    }
}

impl DatabaseMerkle for Database<OffChain> {
    type TableType = OldFuelBlockMerkleData;

    fn merkle_data(&self, version: &u64) -> StorageResult<binary::Primitive> {
        self.storage_as_ref::<OldFuelBlockMerkleData>()
            .get(version)?
            .map(Cow::into_owned)
            .ok_or(not_found!(OldFuelBlockMerkleData))
    }

    fn merkle_metadata(
        &self,
        height: &BlockHeight,
    ) -> StorageResult<DenseMerkleMetadata> {
        self.storage_as_ref::<OldFuelBlockMerkleMetadata>()
            .get(&DenseMetadataKey::Primary(*height))?
            .map(Cow::into_owned)
            .ok_or(not_found!(OldFuelBlockMerkleMetadata))
    }

    fn load_merkle_tree(
        &self,
        version: u64,
    ) -> StorageResult<binary::MerkleTree<OldFuelBlockMerkleData, &Self>> {
        let tree: binary::MerkleTree<OldFuelBlockMerkleData, _> =
            binary::MerkleTree::load(self, version)
                .map_err(|err| StorageError::Other(anyhow::anyhow!(err)))?;
        Ok(tree)
    }
}

impl OffChainDatabase for Database<OffChain> {
    fn block_height(&self, id: &BlockId) -> StorageResult<BlockHeight> {
        self.get_block_height(id)
            .and_then(|height| height.ok_or(not_found!("BlockHeight")))
    }

    fn tx_status(&self, tx_id: &TxId) -> StorageResult<TransactionStatus> {
        self.get_tx_status(tx_id)
            .transpose()
            .ok_or(not_found!("TransactionId"))?
    }

    fn owned_coins_ids(
        &self,
        owner: &Address,
        start_coin: Option<UtxoId>,
        direction: IterDirection,
    ) -> BoxedIter<'_, StorageResult<UtxoId>> {
        self.owned_coins_ids(owner, start_coin, Some(direction))
            .map(|res| res.map_err(StorageError::from))
            .into_boxed()
    }

    fn owned_message_ids(
        &self,
        owner: &Address,
        start_message_id: Option<Nonce>,
        direction: IterDirection,
    ) -> BoxedIter<'_, StorageResult<Nonce>> {
        self.owned_message_ids(owner, start_message_id, Some(direction))
            .map(|result| result.map_err(StorageError::from))
            .into_boxed()
    }

    fn owned_transactions_ids(
        &self,
        owner: Address,
        start: Option<TxPointer>,
        direction: IterDirection,
    ) -> BoxedIter<StorageResult<(TxPointer, TxId)>> {
        let start = start.map(|tx_pointer| OwnedTransactionIndexCursor {
            block_height: tx_pointer.block_height(),
            tx_idx: tx_pointer.tx_index(),
        });
        self.owned_transactions(owner, start, Some(direction))
            .map(|result| result.map_err(StorageError::from))
            .into_boxed()
    }

    fn contract_salt(&self, contract_id: &ContractId) -> StorageResult<Salt> {
        let salt = *self
            .storage_as_ref::<ContractsInfo>()
            .get(contract_id)?
            .ok_or(not_found!(ContractsInfo))?
            .salt();

        Ok(salt)
    }

    fn old_block(&self, height: &BlockHeight) -> StorageResult<CompressedBlock> {
        <Self as DatabaseBlocks>::block(self, height)
    }

    fn old_blocks(
        &self,
        height: Option<BlockHeight>,
        direction: IterDirection,
    ) -> BoxedIter<'_, StorageResult<CompressedBlock>> {
        <Self as DatabaseBlocks>::blocks(self, height, direction)
    }

    fn old_block_consensus(&self, height: &BlockHeight) -> StorageResult<Consensus> {
        <Self as DatabaseBlocks>::consensus(self, height)
    }

    fn old_transaction(&self, id: &TxId) -> StorageResult<Option<Transaction>> {
        self.storage_as_ref::<OldTransactions>()
            .get(id)
            .map(|tx| tx.map(|tx| tx.into_owned()))
    }

    fn relayed_tx_status(
        &self,
        id: Bytes32,
    ) -> StorageResult<Option<RelayedTransactionStatus>> {
        let status = self
            .storage_as_ref::<RelayedTransactionStatuses>()
            .get(&id)
            .map_err(StorageError::from)?
            .map(|cow| cow.into_owned());
        Ok(status)
    }

    fn message_is_spent(&self, nonce: &Nonce) -> StorageResult<bool> {
        self.message_is_spent(nonce)
    }
}

impl Transactional for Database<OffChain> {
    type Transaction<'a> = StorageTransaction<&'a mut Self> where Self: 'a;

    fn transaction(&mut self) -> Self::Transaction<'_> {
        self.into_transaction()
    }
}
