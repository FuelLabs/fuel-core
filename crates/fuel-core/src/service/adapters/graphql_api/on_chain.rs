use crate::{
    database::{
        database_description::on_chain::OnChain,
        Database,
        OnChainIterableKeyValueView,
    },
    fuel_core_graphql_api::ports::{
        DatabaseBlocks,
        DatabaseChain,
        DatabaseContracts,
        DatabaseMessages,
        OnChainDatabase,
    },
    graphql_api::ports::{
        worker,
        DatabaseCoins,
    },
};
use fuel_core_storage::{
    iter::{
        BoxedIterSend,
        BoxedIter,
        IntoBoxedIterSend,
        IntoBoxedIter,
        IterDirection,
        IteratorOverTable,
    },
    not_found,
    tables::{
        Coins,
        FuelBlocks,
        Messages,
        SealedBlockConsensus,
        Transactions,
    },
    Error as StorageError,
    Result as StorageResult,
    StorageAsRef,
    StorageBatchInspect,
};
use fuel_core_types::{
    blockchain::{
        block::CompressedBlock,
        consensus::Consensus,
        primitives::DaBlockHeight,
    },
    entities::{
        coins::coin::Coin,
        relayer::message::Message,
    },
    fuel_tx::{
        AssetId,
        ContractId,
        Transaction,
        TxId,
        UtxoId,
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

    fn transactions<'a>(
        &'a self,
        tx_ids: BoxedIter<'a, &'a TxId>,
    ) -> BoxedIter<'a, StorageResult<Transaction>> {
        <Self as StorageBatchInspect<Transactions>>::get_batch(self, tx_ids)
            .map(|result| result.and_then(|opt| opt.ok_or(not_found!(Transactions))))
            .into_boxed()
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
    ) -> BoxedIterSend<'_, StorageResult<CompressedBlock>> {
        self.iter_all_by_start::<FuelBlocks>(height.as_ref(), Some(direction))
            .map(|result| result.map(|(_, block)| block))
            .into_boxed_send()
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
    ) -> BoxedIterSend<'_, StorageResult<Message>> {
        self.all_messages(start_message_id, Some(direction))
            .map(|result| result.map_err(StorageError::from))
            .into_boxed_send()
    }

    fn message_batch<'a>(
        &'a self,
        ids: BoxedIter<'a, &'a Nonce>,
    ) -> BoxedIter<'a, StorageResult<Message>> {
        <Self as StorageBatchInspect<Messages>>::get_batch(self, ids)
            .map(|result| result.and_then(|opt| opt.ok_or(not_found!(Messages))))
            .into_boxed()
    }

    fn message_exists(&self, nonce: &Nonce) -> StorageResult<bool> {
        self.message_exists(nonce)
    }
}

impl DatabaseCoins for OnChainIterableKeyValueView {
    fn coin(&self, utxo_id: fuel_core_types::fuel_tx::UtxoId) -> StorageResult<Coin> {
        let coin = self
            .storage::<Coins>()
            .get(&utxo_id)?
            .ok_or(not_found!(Coins))?
            .into_owned();

        Ok(coin.uncompress(utxo_id))
    }

    fn coins<'a>(
        &'a self,
        utxo_ids: &'a [UtxoId],
    ) -> BoxedIter<'a, StorageResult<Coin>> {
        <Self as StorageBatchInspect<Coins>>::get_batch(
            self,
            utxo_ids.iter().into_boxed_send(),
        )
        .zip(utxo_ids.iter().into_boxed_send())
        .map(|(res, utxo_id)| {
            res.and_then(|opt| opt.ok_or(not_found!(Coins)))
                .map(|coin| coin.uncompress(*utxo_id))
        })
        .into_boxed()
    }
}

impl DatabaseContracts for OnChainIterableKeyValueView {
    fn contract_balances(
        &self,
        contract: ContractId,
        start_asset: Option<AssetId>,
        direction: IterDirection,
    ) -> BoxedIterSend<StorageResult<ContractBalance>> {
        self.filter_contract_balances(contract, start_asset, Some(direction))
            .map_ok(|entry| ContractBalance {
                owner: *entry.key.contract_id(),
                amount: entry.value,
                asset_id: *entry.key.asset_id(),
            })
            .map(|res| res.map_err(StorageError::from))
            .into_boxed_send()
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
