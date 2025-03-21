use crate::{
    database::{
        database_description::on_chain::OnChain,
        Database,
        OnChainIterableKeyValueView,
        OnChainKeyValueView,
    },
    fuel_core_graphql_api::ports::{
        DatabaseBlocks,
        DatabaseChain,
        DatabaseContracts,
        DatabaseMessages,
        OnChainDatabase,
        OnChainDatabaseAt,
    },
    graphql_api::ports::worker,
};
use fuel_core_storage::{
    iter::{
        BoxedIter,
        IntoBoxedIter,
        IterDirection,
        IteratorOverTable,
    },
    not_found,
    tables::{
        ContractsAssets,
        ContractsState,
        FuelBlocks,
        SealedBlockConsensus,
        Transactions,
    },
    ContractsAssetKey,
    ContractsStateKey,
    Result as StorageResult,
    StorageAsRef,
};
use fuel_core_types::{
    blockchain::{
        block::CompressedBlock,
        consensus::Consensus,
        primitives::DaBlockHeight,
    },
    entities::relayer::message::Message,
    fuel_tx::{
        AssetId,
        Bytes32,
        ContractId,
        Transaction,
        TxId,
    },
    fuel_types::{
        BlockHeight,
        Nonce,
    },
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
            .into_boxed()
    }

    fn contract_storage_slots(
        &self,
        contract: ContractId,
    ) -> BoxedIter<StorageResult<(Bytes32, Vec<u8>)>> {
        self.iter_all_by_prefix::<ContractsState, _>(Some(contract))
            .map(|res| res.map(|(key, value)| (*key.state_key(), value.0)))
            .into_boxed()
    }

    fn contract_storage_balances(
        &self,
        contract: ContractId,
    ) -> BoxedIter<StorageResult<ContractBalance>> {
        self.iter_all_by_prefix::<ContractsAssets, _>(Some(contract))
            .map(|res| {
                res.map(|(key, value)| ContractBalance {
                    owner: *key.contract_id(),
                    amount: value,
                    asset_id: *key.asset_id(),
                })
            })
            .into_boxed()
    }
}

impl DatabaseChain for OnChainIterableKeyValueView {
    fn da_height(&self) -> StorageResult<DaBlockHeight> {
        self.latest_compressed_block()?
            .map(|block| block.header().da_height())
            .ok_or(not_found!("DaBlockHeight"))
    }
}

impl OnChainDatabase for OnChainIterableKeyValueView {}

impl worker::OnChainDatabase for Database<OnChain> {
    fn latest_height(&self) -> StorageResult<Option<BlockHeight>> {
        Ok(fuel_core_storage::transactional::HistoricalView::latest_height(self))
    }
}

impl OnChainDatabaseAt for OnChainKeyValueView {
    fn contract_slot_values(
        &self,
        contract_id: ContractId,
        storage_slots: Vec<Bytes32>,
    ) -> BoxedIter<StorageResult<(Bytes32, Vec<u8>)>> {
        storage_slots
            .into_iter()
            .map(move |key| {
                let double_key = ContractsStateKey::new(&contract_id, &key);
                let value = self
                    .storage::<ContractsState>()
                    .get(&double_key)?
                    .map(|v| v.into_owned().0);

                Ok(value.map(|v| (key, v)))
            })
            .filter_map(|res| res.transpose())
            .into_boxed()
    }

    fn contract_balance_values(
        &self,
        contract_id: ContractId,
        assets: Vec<AssetId>,
    ) -> BoxedIter<StorageResult<ContractBalance>> {
        assets
            .into_iter()
            .map(move |asset| {
                let double_key = ContractsAssetKey::new(&contract_id, &asset);
                let value = self
                    .storage::<ContractsAssets>()
                    .get(&double_key)?
                    .map(|v| v.into_owned());

                Ok(value.map(|v| ContractBalance {
                    owner: contract_id,
                    amount: v,
                    asset_id: asset,
                }))
            })
            .filter_map(|res| res.transpose())
            .into_boxed()
    }
}
