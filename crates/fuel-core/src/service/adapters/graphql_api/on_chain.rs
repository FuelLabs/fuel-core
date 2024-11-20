use crate::{
    database::{
        database_description::on_chain::OnChain, Database, OnChainIterableKeyValueView,
    },
    fuel_core_graphql_api::ports::{
        DatabaseBlocks, DatabaseChain, DatabaseContracts, DatabaseMessages,
        OnChainDatabase,
    },
    graphql_api::ports::worker,
};
use fuel_core_storage::{
    iter::{BoxedIter, IntoBoxedIter, IterDirection, IteratorOverTable},
    not_found,
    tables::{FuelBlocks, SealedBlockConsensus, Transactions},
    Error as StorageError, Result as StorageResult, StorageAsRef,
};
use fuel_core_types::{
    blockchain::{
        block::CompressedBlock, consensus::Consensus, primitives::DaBlockHeight,
    },
    entities::relayer::message::Message,
    fuel_tx::{AssetId, ContractId, Transaction, TxId},
    fuel_types::{BlockHeight, Nonce},
    services::graphql_api::ContractBalance,
};
use itertools::Itertools;

impl DatabaseBlocks for OnChainIterableKeyValueView {
    fn transaction(&self, tx_id: &TxId) -> StorageResult<Transaction> {
        Ok(self
            .storage::<Transactions>()
            .get(tx_id)?
            .ok_or(not_found!(Transactions))?
            .into_owned())
    }

    fn block(&self, height: &BlockHeight) -> StorageResult<CompressedBlock> {
        let block = self
            .storage_as_ref::<FuelBlocks>()
            .get(height)?
            .ok_or_else(|| not_found!(FuelBlocks))?
            .into_owned();

        Ok(block)
    }

    fn blocks(
        &self,
        height: Option<BlockHeight>,
        direction: IterDirection,
    ) -> BoxedIter<'_, StorageResult<CompressedBlock>> {
        self.iter_all_by_start::<FuelBlocks>(height.as_ref(), Some(direction))
            .map(|result| result.map(|(_, block)| block))
            .into_boxed()
    }

    fn latest_height(&self) -> StorageResult<BlockHeight> {
        self.latest_height()
    }

    fn consensus(&self, id: &BlockHeight) -> StorageResult<Consensus> {
        self.storage_as_ref::<SealedBlockConsensus>()
            .get(id)
            .map(|c| c.map(|c| c.into_owned()))?
            .ok_or(not_found!(SealedBlockConsensus))
    }
}

impl DatabaseMessages for OnChainIterableKeyValueView {
    fn all_messages(
        &self,
        start_message_id: Option<Nonce>,
        direction: IterDirection,
    ) -> BoxedIter<'_, StorageResult<Message>> {
        self.all_messages(start_message_id, Some(direction))
            .map(|result| result.map_err(StorageError::from))
            .into_boxed()
    }

    fn message_exists(&self, nonce: &Nonce) -> StorageResult<bool> {
        self.message_exists(nonce)
    }
}

impl DatabaseContracts for OnChainIterableKeyValueView {
    fn contract_balances(
        &self,
        contract: ContractId,
        start_asset: Option<AssetId>,
        direction: IterDirection,
    ) -> BoxedIter<StorageResult<ContractBalance>> {
        self.filter_contract_balances(contract, start_asset, Some(direction))
            .map_ok(|entry| ContractBalance {
                owner: *entry.key.contract_id(),
                amount: entry.value,
                asset_id: *entry.key.asset_id(),
            })
            .map(|res| res.map_err(StorageError::from))
            .into_boxed()
    }
}

impl DatabaseChain for OnChainIterableKeyValueView {
    fn da_height(&self) -> StorageResult<DaBlockHeight> {
        self.latest_compressed_block()?
            .map(|block| block.header().da_height)
            .ok_or(not_found!("DaBlockHeight"))
    }
}

impl OnChainDatabase for OnChainIterableKeyValueView {}

impl worker::OnChainDatabase for Database<OnChain> {
    fn latest_height(&self) -> StorageResult<Option<BlockHeight>> {
        Ok(fuel_core_storage::transactional::HistoricalView::latest_height(self))
    }
}
