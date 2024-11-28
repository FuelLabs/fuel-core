use std::borrow::Cow;

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
    graphql_api::{
        indexation::coins_to_spend::{
            IndexedCoinType,
            NON_RETRYABLE_BYTE,
        },
        storage::{
            balances::{
                CoinBalances,
                CoinBalancesKey,
                MessageBalance,
                MessageBalances,
                TotalBalanceAmount,
            },
            coins::{
                CoinsToSpendIndex,
                CoinsToSpendIndexKey,
            },
            old::{
                OldFuelBlockConsensus,
                OldFuelBlocks,
                OldTransactions,
            },
            Column,
        },
    },
    state::IterableKeyValueView,
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
    StorageRef,
};
use fuel_core_types::{
    blockchain::{
        block::CompressedBlock,
        consensus::Consensus,
        primitives::BlockId,
    },
    entities::{
        coins::CoinType,
        relayer::transaction::RelayedTransactionStatus,
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
        BlockHeight,
        Nonce,
    },
    services::txpool::TransactionStatus,
};
use rand::Rng;

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
            .map(|value| value.as_ref().clone())
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

    fn balances(
        &self,
        owner: &Address,
        base_asset_id: &AssetId,
        direction: IterDirection,
    ) -> BoxedIter<'_, StorageResult<(AssetId, TotalBalanceAmount)>> {
        let base_asset_id = *base_asset_id;
        let base_asset_balance = base_asset_balance(
            self.storage_as_ref::<MessageBalances>(),
            self.storage_as_ref::<CoinBalances>(),
            owner,
            &base_asset_id,
        );

        let non_base_asset_balance = self
            .iter_all_filtered_keys::<CoinBalances, _>(Some(owner), None, Some(direction))
            .filter_map(move |result| match result {
                Ok(key) if *key.asset_id() != base_asset_id => Some(Ok(key)),
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
            .into_boxed();

        if direction == IterDirection::Forward {
            base_asset_balance
                .chain(non_base_asset_balance)
                .into_boxed()
        } else {
            non_base_asset_balance
                .chain(base_asset_balance)
                .into_boxed()
        }
    }

    fn coins_to_spend(
        &self,
        owner: &Address,
        asset_id: &AssetId,
        target_amount: u64,
        max_coins: u32,
    ) -> StorageResult<Vec<(Vec<u8>, IndexedCoinType)>> {
        let prefix: Vec<_> = owner
            .as_ref()
            .iter()
            .copied()
            .chain(asset_id.as_ref().iter().copied())
            .chain(NON_RETRYABLE_BYTE)
            .collect();

        let big_first_iter = self.iter_all_filtered::<CoinsToSpendIndex, _>(
            Some(&prefix),
            None,
            Some(IterDirection::Reverse),
        );

        let dust_first_iter = self.iter_all_filtered::<CoinsToSpendIndex, _>(
            Some(&prefix),
            None,
            Some(IterDirection::Forward),
        );

        let selected_iter =
            select_1(big_first_iter, dust_first_iter, target_amount, max_coins);

        Ok(selected_iter
            .map(|x| x.unwrap())
            .map(|(key, value)| {
                let foreign_key = key.foreign_key_bytes().to_vec();
                let coin_type = IndexedCoinType::try_from(value).unwrap();
                (foreign_key, coin_type)
            })
            .collect())
    }
}

fn select_1<'a>(
    coins_iter: BoxedIter<Result<(CoinsToSpendIndexKey, u8), StorageError>>,
    coins_iter_back: BoxedIter<Result<(CoinsToSpendIndexKey, u8), StorageError>>,
    total: u64,
    max: u32,
) -> BoxedIter<'a, Result<(CoinsToSpendIndexKey, u8), StorageError>> {
    // TODO[RC]: Validate query parameters.
    if total == 0 && max == 0 {
        return std::iter::empty().into_boxed();
    }

    let (selected_big_coins_total, selected_big_coins) =
        big_coins(coins_iter, total, max);
    dbg!(&selected_big_coins_total);
    dbg!(&total);
    if selected_big_coins_total < total {
        dbg!(1);
        return std::iter::empty().into_boxed();
    }
    dbg!(2);
    let Some(last_selected_big_coin) = selected_big_coins.last() else {
        // Should never happen.
        dbg!(3);
        return std::iter::empty().into_boxed();
    };

    let max_dust_count = max_dust_count(max, selected_big_coins.len());
    dbg!(&max_dust_count);
    let (dust_coins_total, dust_coins) =
        dust_coins(coins_iter_back, last_selected_big_coin, max_dust_count);

    let retained_big_coins_iter =
        skip_big_coins_up_to_amount(selected_big_coins, dust_coins_total);
    (retained_big_coins_iter.chain(dust_coins)).into_boxed()
}

fn big_coins<'a>(
    coins_iter: BoxedIter<Result<(CoinsToSpendIndexKey, u8), StorageError>>,
    total: u64,
    max: u32,
) -> (u64, Vec<Result<(CoinsToSpendIndexKey, u8), StorageError>>) {
    let mut big_coins_total = 0;
    let big_coins: Vec<_> = coins_iter
        .take(max as usize)
        .take_while(|item| {
            (big_coins_total >= total)
                .then_some(false)
                .unwrap_or_else(|| {
                    big_coins_total =
                        big_coins_total.saturating_add(item.as_ref().unwrap().0.amount());
                    true
                })
        })
        .collect();
    (big_coins_total, big_coins)
}

fn max_dust_count(max: u32, big_coins_len: usize) -> u32 {
    let mut rng = rand::thread_rng();
    rng.gen_range(0..=max.saturating_sub(big_coins_len as u32))
}

fn dust_coins<'a>(
    coins_iter_back: BoxedIter<Result<(CoinsToSpendIndexKey, u8), StorageError>>,
    last_big_coin: &'a Result<(CoinsToSpendIndexKey, u8), StorageError>,
    max_dust_count: u32,
) -> (u64, Vec<Result<(CoinsToSpendIndexKey, u8), StorageError>>) {
    let mut dust_coins_total = 0;
    let dust_coins: Vec<_> = coins_iter_back
        .take(max_dust_count as usize)
        .take_while(move |item| item != last_big_coin)
        .map(|item| {
            dust_coins_total += item.as_ref().unwrap().0.amount() as u64;
            item
        })
        .collect();
    (dust_coins_total, dust_coins)
}

fn skip_big_coins_up_to_amount<'a>(
    big_coins: impl IntoIterator<Item = Result<(CoinsToSpendIndexKey, u8), StorageError>>,
    mut dust_coins_total: u64,
) -> impl Iterator<Item = Result<(CoinsToSpendIndexKey, u8), StorageError>> {
    big_coins.into_iter().skip_while(move |item| {
        dust_coins_total
            .checked_sub(item.as_ref().unwrap().0.amount())
            .and_then(|new_value| {
                dust_coins_total = new_value;
                Some(true)
            })
            .unwrap_or_default()
    })
}

struct AssetBalanceWithRetrievalErrors<'a> {
    balance: Option<u128>,
    errors: BoxedIter<'a, Result<(AssetId, u128), StorageError>>,
}

impl<'a> AssetBalanceWithRetrievalErrors<'a> {
    fn new(
        balance: Option<u128>,
        errors: BoxedIter<'a, Result<(AssetId, u128), StorageError>>,
    ) -> Self {
        Self { balance, errors }
    }
}

fn non_retryable_message_balance<'a>(
    storage: StorageRef<'a, IterableKeyValueView<Column>, MessageBalances>,
    owner: &Address,
) -> AssetBalanceWithRetrievalErrors<'a> {
    storage.get(owner).map_or_else(
        |err| {
            AssetBalanceWithRetrievalErrors::new(
                None,
                std::iter::once(Err(err)).into_boxed(),
            )
        },
        |value| {
            AssetBalanceWithRetrievalErrors::new(
                value.map(|value| value.non_retryable),
                std::iter::empty().into_boxed(),
            )
        },
    )
}

fn base_asset_coin_balance<'a, 'b>(
    storage: StorageRef<'a, IterableKeyValueView<Column>, CoinBalances>,
    owner: &'b Address,
    base_asset_id: &'b AssetId,
) -> AssetBalanceWithRetrievalErrors<'a> {
    storage
        .get(&CoinBalancesKey::new(owner, base_asset_id))
        .map_or_else(
            |err| {
                AssetBalanceWithRetrievalErrors::new(
                    None,
                    std::iter::once(Err(err)).into_boxed(),
                )
            },
            |value| {
                AssetBalanceWithRetrievalErrors::new(
                    value.map(Cow::into_owned),
                    std::iter::empty().into_boxed(),
                )
            },
        )
}

fn base_asset_balance<'a, 'b>(
    messages_storage: StorageRef<'a, IterableKeyValueView<Column>, MessageBalances>,
    coins_storage: StorageRef<'a, IterableKeyValueView<Column>, CoinBalances>,
    owner: &'b Address,
    base_asset_id: &'b AssetId,
) -> BoxedIter<'a, Result<(AssetId, TotalBalanceAmount), StorageError>> {
    let AssetBalanceWithRetrievalErrors {
        balance: messages_balance,
        errors: message_errors,
    } = non_retryable_message_balance(messages_storage, owner);

    let AssetBalanceWithRetrievalErrors {
        balance: base_coin_balance,
        errors: coin_errors,
    } = base_asset_coin_balance(coins_storage, owner, base_asset_id);

    let base_asset_id = *base_asset_id;
    let balance = match (messages_balance, base_coin_balance) {
        (None, None) => None,
        (None, Some(balance)) | (Some(balance), None) => Some(balance),
        (Some(msg_balance), Some(coin_balance)) => {
            Some(msg_balance.saturating_add(coin_balance))
        }
    }
    .into_iter()
    .map(move |balance| Ok((base_asset_id, balance)));

    message_errors
        .chain(coin_errors)
        .chain(balance)
        .into_boxed()
}

impl worker::OffChainDatabase for Database<OffChain> {
    type Transaction<'a> = StorageTransaction<&'a mut Self> where Self: 'a;

    fn latest_height(&self) -> StorageResult<Option<BlockHeight>> {
        Ok(fuel_core_storage::transactional::HistoricalView::latest_height(self))
    }

    fn transaction(&mut self) -> Self::Transaction<'_> {
        self.into_transaction()
    }

    fn balances_indexation_enabled(&self) -> StorageResult<bool> {
        self.indexation_available(IndexationKind::Balances)
    }

    fn coins_to_spend_indexation_enabled(&self) -> StorageResult<bool> {
        self.indexation_available(IndexationKind::CoinsToSpend)
    }
}
