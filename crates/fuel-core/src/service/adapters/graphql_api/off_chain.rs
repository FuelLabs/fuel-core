use crate::{
    database::{
        database_description::{
            off_chain::OffChain,
            IndexationKind,
        },
        Database,
        OffChainIterableKeyValueView,
    },
    fuel_core_graphql_api::{
        ports::{
            worker,
            OffChainDatabase,
        },
        storage::{
            contracts::ContractsInfo,
            da_compression::DaCompressedBlocks,
            relayed_transactions::RelayedTransactionStatuses,
            transactions::OwnedTransactionIndexCursor,
        },
    },
    graphql_api::storage::{
        balances::{
            CoinBalances,
            CoinBalancesKey,
            MessageBalance,
            MessageBalances,
            TotalBalanceAmount,
        },
        old::{
            OldFuelBlockConsensus,
            OldFuelBlocks,
            OldTransactions,
        },
    },
};
use fuel_core_storage::{
    blueprint::BlueprintInspect,
    codec::Encode,
    iter::{
        BoxedIter,
        IntoBoxedIter,
        IterDirection,
        IteratorOverTable,
    },
    kv_store::KeyValueInspect,
    not_found,
    structured_storage::TableWithBlueprint,
    transactional::{
        IntoTransaction,
        StorageTransaction,
    },
    Error as StorageError,
    Result as StorageResult,
    StorageAsRef,
};
use fuel_core_types::{
    blockchain::{
        block::CompressedBlock,
        consensus::Consensus,
        primitives::BlockId,
    },
    entities::relayer::transaction::RelayedTransactionStatus,
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
        BlockHeight,
        Nonce,
    },
    services::txpool::TransactionStatus,
};
use std::iter;

impl OffChainDatabase for OffChainIterableKeyValueView {
    fn block_height(&self, id: &BlockId) -> StorageResult<BlockHeight> {
        self.get_block_height(id)
            .and_then(|height| height.ok_or(not_found!("BlockHeight")))
    }

    fn da_compressed_block(&self, height: &BlockHeight) -> StorageResult<Vec<u8>> {
        let column = <DaCompressedBlocks as TableWithBlueprint>::column();
        let encoder =
            <<DaCompressedBlocks as TableWithBlueprint>::Blueprint as BlueprintInspect<
                DaCompressedBlocks,
                Self,
            >>::KeyCodec::encode(height);

        self.get(encoder.as_ref(), column)?
            .ok_or_else(|| not_found!(DaCompressedBlocks))
            .map(|value| value.to_vec())
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
        let block = self
            .storage_as_ref::<OldFuelBlocks>()
            .get(height)?
            .ok_or(not_found!(OldFuelBlocks))?
            .into_owned();

        Ok(block)
    }

    fn old_blocks(
        &self,
        height: Option<BlockHeight>,
        direction: IterDirection,
    ) -> BoxedIter<'_, StorageResult<CompressedBlock>> {
        self.iter_all_by_start::<OldFuelBlocks>(height.as_ref(), Some(direction))
            .map(|r| r.map(|(_, block)| block))
            .into_boxed()
    }

    fn old_block_consensus(&self, height: &BlockHeight) -> StorageResult<Consensus> {
        Ok(self
            .storage_as_ref::<OldFuelBlockConsensus>()
            .get(height)?
            .ok_or(not_found!(OldFuelBlockConsensus))?
            .into_owned())
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

    fn balance(
        &self,
        owner: &Address,
        asset_id: &AssetId,
        base_asset_id: &AssetId,
    ) -> StorageResult<TotalBalanceAmount> {
        let coins = self
            .storage_as_ref::<CoinBalances>()
            .get(&CoinBalancesKey::new(owner, asset_id))?
            .unwrap_or_default()
            .into_owned() as TotalBalanceAmount;

        if base_asset_id == asset_id {
            let MessageBalance {
                retryable: _, // TODO: https://github.com/FuelLabs/fuel-core/issues/2448
                non_retryable,
            } = self
                .storage_as_ref::<MessageBalances>()
                .get(owner)?
                .unwrap_or_default()
                .into_owned();

            let total = coins.checked_add(non_retryable).ok_or(anyhow::anyhow!(
                "Total balance overflow: coins: {coins}, messages: {non_retryable}"
            ))?;
            Ok(total)
        } else {
            Ok(coins)
        }
    }

    fn balances<'a>(
        &'a self,
        owner: &Address,
        start: Option<AssetId>,
        base_asset_id: &'a AssetId,
        direction: IterDirection,
    ) -> BoxedIter<'a, StorageResult<(AssetId, TotalBalanceAmount)>> {
        match (direction, start) {
            (IterDirection::Forward, None) => {
                let base_asset_balance = self.base_asset_balance(base_asset_id, owner);
                let non_base_asset_balance =
                    self.non_base_asset_balance(owner, None, direction, base_asset_id);
                base_asset_balance
                    .chain(non_base_asset_balance)
                    .into_boxed()
            }
            (IterDirection::Forward, Some(asset_id)) => {
                let start = (asset_id != *base_asset_id)
                    .then_some(CoinBalancesKey::new(owner, &asset_id));
                self.non_base_asset_balance(owner, start, direction, base_asset_id)
            }
            (IterDirection::Reverse, _) => {
                let start = start.map(|asset_id| CoinBalancesKey::new(owner, &asset_id));
                let base_asset_balance = self.base_asset_balance(base_asset_id, owner);
                let non_base_asset_balance =
                    self.non_base_asset_balance(owner, start, direction, base_asset_id);
                non_base_asset_balance
                    .chain(base_asset_balance)
                    .into_boxed()
            }
        }
    }
}

impl OffChainIterableKeyValueView {
    fn base_asset_balance(
        &self,
        base_asset_id: &AssetId,
        owner: &Address,
    ) -> BoxedIter<'_, Result<(AssetId, u128), StorageError>> {
        let base_asset_id = *base_asset_id;
        let base_balance = self.balance(owner, &base_asset_id, &base_asset_id);
        match base_balance {
            Ok(base_asset_balance) => {
                if base_asset_balance != 0 {
                    iter::once(Ok((base_asset_id, base_asset_balance))).into_boxed()
                } else {
                    iter::empty().into_boxed()
                }
            }
            Err(err) => iter::once(Err(err)).into_boxed(),
        }
    }

    fn non_base_asset_balance<'a>(
        &'a self,
        owner: &Address,
        start: Option<CoinBalancesKey>,
        direction: IterDirection,
        base_asset_id: &'a AssetId,
    ) -> BoxedIter<'_, Result<(AssetId, u128), StorageError>> {
        self.iter_all_filtered_keys::<CoinBalances, _>(
            Some(owner),
            start.as_ref(),
            Some(direction),
        )
        .filter_map(move |result| match result {
            Ok(key) if *key.asset_id() != *base_asset_id => Some(Ok(key)),
            Ok(_) => None,
            Err(err) => Some(Err(err)),
        })
        .map(move |result| {
            result.and_then(|key| {
                let asset_id = key.asset_id();
                let coin_balance =
                    self.storage_as_ref::<CoinBalances>()
                        .get(&key)?
                        .unwrap_or_default()
                        .into_owned() as TotalBalanceAmount;
                Ok((*asset_id, coin_balance))
            })
        })
        .into_boxed()
    }
}

impl worker::OffChainDatabase for Database<OffChain> {
    type Transaction<'a> = StorageTransaction<&'a mut Self> where Self: 'a;

    fn latest_height(&self) -> StorageResult<Option<BlockHeight>> {
        Ok(fuel_core_storage::transactional::HistoricalView::latest_height(self))
    }

    fn transaction(&mut self) -> Self::Transaction<'_> {
        self.into_transaction()
    }

    fn balances_enabled(&self) -> StorageResult<bool> {
        self.indexation_available(IndexationKind::Balances)
    }
}
