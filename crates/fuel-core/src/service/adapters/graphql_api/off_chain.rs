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
                COIN_FOREIGN_KEY_LEN,
                MESSAGE_FOREIGN_KEY_LEN,
            },
            old::{
                OldFuelBlockConsensus,
                OldFuelBlocks,
                OldTransactions,
            },
        },
    },
    schema::coins::{
        CoinOrMessageIdBytes,
        ExcludedKeysAsBytes,
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
    entities::{
        coins::CoinId,
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
use std::iter;

type CoinsToSpendIndexEntry = (CoinsToSpendIndexKey, u8);

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

    fn balances(
        &self,
        owner: &Address,
        base_asset_id: &AssetId,
        direction: IterDirection,
    ) -> BoxedIter<'_, StorageResult<(AssetId, TotalBalanceAmount)>> {
        let base_asset_id = *base_asset_id;
        let base_balance = self.balance(owner, &base_asset_id, &base_asset_id);
        let base_asset_balance = match base_balance {
            Ok(base_asset_balance) => {
                if base_asset_balance != 0 {
                    iter::once(Ok((base_asset_id, base_asset_balance))).into_boxed()
                } else {
                    iter::empty().into_boxed()
                }
            }
            Err(err) => iter::once(Err(err)).into_boxed(),
        };

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
        max_coins: u16,
        excluded_ids: &ExcludedKeysAsBytes,
    ) -> StorageResult<Vec<CoinId>> {
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

        let selected_iter = select_coins_to_spend(
            big_first_iter,
            dust_first_iter,
            target_amount,
            max_coins,
            excluded_ids,
        )?;

        into_coin_id(selected_iter, max_coins as usize)
    }
}

fn into_coin_id(
    selected_iter: Vec<(CoinsToSpendIndexKey, u8)>,
    max_coins: usize,
) -> Result<Vec<CoinId>, StorageError> {
    let mut coins = Vec::with_capacity(max_coins);
    for (foreign_key, coin_type) in selected_iter {
        let coin_type =
            IndexedCoinType::try_from(coin_type).map_err(StorageError::from)?;
        let coin = match coin_type {
            IndexedCoinType::Coin => {
                let bytes: [u8; COIN_FOREIGN_KEY_LEN] = foreign_key
                    .foreign_key_bytes()
                    .as_slice()
                    .try_into()
                    .map_err(StorageError::from)?;

                let (tx_id_bytes, output_index_bytes) = bytes.split_at(TxId::LEN);
                let tx_id = TxId::try_from(tx_id_bytes).map_err(StorageError::from)?;
                let output_index = u16::from_be_bytes(
                    output_index_bytes.try_into().map_err(StorageError::from)?,
                );
                CoinId::Utxo(UtxoId::new(tx_id, output_index))
            }
            IndexedCoinType::Message => {
                let bytes: [u8; MESSAGE_FOREIGN_KEY_LEN] = foreign_key
                    .foreign_key_bytes()
                    .as_slice()
                    .try_into()
                    .map_err(StorageError::from)?;
                let nonce = Nonce::from(bytes);
                CoinId::Message(nonce)
            }
        };
        coins.push(coin);
    }
    Ok(coins)
}

fn select_coins_to_spend(
    big_coins_iter: BoxedIter<StorageResult<CoinsToSpendIndexEntry>>,
    dust_coins_iter: BoxedIter<StorageResult<CoinsToSpendIndexEntry>>,
    total: u64,
    max: u16,
    excluded_ids: &ExcludedKeysAsBytes,
) -> StorageResult<Vec<CoinsToSpendIndexEntry>> {
    if total == 0 && max == 0 {
        return Ok(vec![]);
    }

    let (selected_big_coins_total, selected_big_coins) =
        big_coins(big_coins_iter, total, max, excluded_ids)?;

    if selected_big_coins_total < total {
        return Ok(vec![]);
    }
    let Some(last_selected_big_coin) = selected_big_coins.last() else {
        // Should never happen.
        return Ok(vec![]);
    };

    let number_of_big_coins: u16 = selected_big_coins
        .len()
        .try_into()
        .map_err(anyhow::Error::from)?;

    let max_dust_count = max_dust_count(max, number_of_big_coins);
    let (dust_coins_total, selected_dust_coins) = dust_coins(
        dust_coins_iter,
        last_selected_big_coin,
        max_dust_count,
        excluded_ids,
    )?;
    let retained_big_coins_iter =
        skip_big_coins_up_to_amount(selected_big_coins, dust_coins_total);

    Ok((retained_big_coins_iter.chain(selected_dust_coins)).collect())
}

fn big_coins(
    coins_iter: BoxedIter<StorageResult<CoinsToSpendIndexEntry>>,
    total: u64,
    max: u16,
    excluded_ids: &ExcludedKeysAsBytes,
) -> StorageResult<(u64, Vec<CoinsToSpendIndexEntry>)> {
    select_coins_until(coins_iter, max, excluded_ids, |_, total_so_far| {
        total_so_far >= total
    })
}

fn dust_coins(
    coins_iter_back: BoxedIter<StorageResult<CoinsToSpendIndexEntry>>,
    last_big_coin: &CoinsToSpendIndexEntry,
    max_dust_count: u16,
    excluded_ids: &ExcludedKeysAsBytes,
) -> StorageResult<(u64, Vec<CoinsToSpendIndexEntry>)> {
    select_coins_until(coins_iter_back, max_dust_count, excluded_ids, |coin, _| {
        coin == last_big_coin
    })
}

fn select_coins_until<F>(
    coins_iter: BoxedIter<StorageResult<CoinsToSpendIndexEntry>>,
    max: u16,
    excluded_ids: &ExcludedKeysAsBytes,
    predicate: F,
) -> StorageResult<(u64, Vec<CoinsToSpendIndexEntry>)>
where
    F: Fn(&CoinsToSpendIndexEntry, u64) -> bool,
{
    let mut coins_total_value: u64 = 0;
    let mut count = 0;
    let mut coins = Vec::with_capacity(max as usize);
    for coin in coins_iter {
        let coin = coin?;
        if !is_excluded(&coin, excluded_ids)? {
            if count >= max || predicate(&coin, coins_total_value) {
                break;
            }
            count = count.saturating_add(1);
            let amount = coin.0.amount();
            coins_total_value = coins_total_value.saturating_add(amount);
            coins.push(coin);
        }
    }
    Ok((coins_total_value, coins))
}

fn is_excluded(
    (key, value): &CoinsToSpendIndexEntry,
    excluded_ids: &ExcludedKeysAsBytes,
) -> StorageResult<bool> {
    let coin_type = IndexedCoinType::try_from(*value).map_err(StorageError::from)?;
    match coin_type {
        IndexedCoinType::Coin => {
            let foreign_key = CoinOrMessageIdBytes::Coin(
                key.foreign_key_bytes()
                    .as_slice()
                    .try_into()
                    .map_err(StorageError::from)?,
            );
            Ok(excluded_ids.coins().contains(&foreign_key))
        }
        IndexedCoinType::Message => {
            let foreign_key = CoinOrMessageIdBytes::Message(
                key.foreign_key_bytes()
                    .as_slice()
                    .try_into()
                    .map_err(StorageError::from)?,
            );
            Ok(excluded_ids.messages().contains(&foreign_key))
        }
    }
}

fn max_dust_count(max: u16, big_coins_len: u16) -> u16 {
    let mut rng = rand::thread_rng();
    rng.gen_range(0..=max.saturating_sub(big_coins_len))
}

fn skip_big_coins_up_to_amount(
    big_coins: impl IntoIterator<Item = CoinsToSpendIndexEntry>,
    mut dust_coins_total: u64,
) -> impl Iterator<Item = CoinsToSpendIndexEntry> {
    big_coins.into_iter().skip_while(move |item| {
        let amount = item.0.amount();
        dust_coins_total
            .checked_sub(amount)
            .map(|new_value| {
                dust_coins_total = new_value;
                true
            })
            .unwrap_or_default()
    })
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

#[cfg(test)]
mod tests {
    use fuel_core_storage::{
        codec::primitive::utxo_id_to_bytes,
        iter::IntoBoxedIter,
        Result as StorageResult,
    };
    use fuel_core_types::{
        entities::coins::coin::Coin,
        fuel_tx::{
            TxId,
            UtxoId,
        },
    };

    use crate::{
        graphql_api::{
            indexation::coins_to_spend::IndexedCoinType,
            storage::coins::CoinsToSpendIndexKey,
        },
        schema::coins::{
            CoinOrMessageIdBytes,
            ExcludedKeysAsBytes,
        },
        service::adapters::graphql_api::off_chain::{
            select_coins_to_spend,
            CoinsToSpendIndexEntry,
        },
    };

    use super::select_coins_until;

    fn setup_test_coins(
        coins: impl IntoIterator<Item = u8>,
    ) -> Vec<Result<(CoinsToSpendIndexKey, u8), fuel_core_storage::Error>> {
        let coins: Vec<StorageResult<_>> = coins
            .into_iter()
            .map(|i| {
                let tx_id: TxId = [i; 32].into();
                let output_index = i as u16;
                let utxo_id = UtxoId::new(tx_id, output_index);

                let coin = Coin {
                    utxo_id,
                    owner: Default::default(),
                    amount: i as u64,
                    asset_id: Default::default(),
                    tx_pointer: Default::default(),
                };

                let entry = (
                    CoinsToSpendIndexKey::from_coin(&coin),
                    IndexedCoinType::Coin as u8,
                );
                Ok(entry)
            })
            .collect();
        coins
    }

    #[test]
    fn select_coins_until_respects_max() {
        const MAX: u16 = 3;

        let coins = setup_test_coins([1, 2, 3, 4, 5]);

        let excluded = ExcludedKeysAsBytes::new(vec![], vec![]);

        let result =
            select_coins_until(coins.into_iter().into_boxed(), MAX, &excluded, |_, _| {
                false
            })
            .expect("should select coins");

        assert_eq!(result.0, 1 + 2 + 3); // Limit is set at 3 coins
        assert_eq!(result.1.len(), 3);
    }

    #[test]
    fn select_coins_until_respects_excluded_ids() {
        const MAX: u16 = u16::MAX;

        let coins = setup_test_coins([1, 2, 3, 4, 5]);

        // Exclude coin with amount '2'.
        let excluded_coin_bytes = {
            let tx_id: TxId = [2; 32].into();
            let output_index = 2;
            let utxo_id = UtxoId::new(tx_id, output_index);
            CoinOrMessageIdBytes::Coin(utxo_id_to_bytes(&utxo_id))
        };
        let excluded = ExcludedKeysAsBytes::new(vec![excluded_coin_bytes], vec![]);

        let result =
            select_coins_until(coins.into_iter().into_boxed(), MAX, &excluded, |_, _| {
                false
            })
            .expect("should select coins");

        assert_eq!(result.0, 1 + 3 + 4 + 5); // '2' is skipped.
        assert_eq!(result.1.len(), 4);
    }

    #[test]
    fn select_coins_until_respects_predicate() {
        const MAX: u16 = u16::MAX;
        const TOTAL: u64 = 7;

        let coins = setup_test_coins([1, 2, 3, 4, 5]);

        let excluded = ExcludedKeysAsBytes::new(vec![], vec![]);

        let predicate: fn(&CoinsToSpendIndexEntry, u64) -> bool =
            |_, total| total > TOTAL;

        let result =
            select_coins_until(coins.into_iter().into_boxed(), MAX, &excluded, predicate)
                .expect("should select coins");

        assert_eq!(result.0, 1 + 2 + 3 + 4); // Keep selecting until total is greater than 7.
        assert_eq!(result.1.len(), 4);
    }

    #[test]
    fn already_selected_big_coins_are_never_reselected_as_dust() {
        const MAX: u16 = u16::MAX;
        const TOTAL: u64 = 101;

        let big_coins_iter = setup_test_coins([100, 4, 3, 2]).into_iter().into_boxed();
        let dust_coins_iter = setup_test_coins([100, 4, 3, 2])
            .into_iter()
            .rev()
            .into_boxed();

        let excluded = ExcludedKeysAsBytes::new(vec![], vec![]);

        let result =
            select_coins_to_spend(big_coins_iter, dust_coins_iter, TOTAL, MAX, &excluded)
                .expect("should select coins");

        let mut results = result
            .into_iter()
            .map(|(key, _)| key.amount())
            .collect::<Vec<_>>();

        // Because we select a total of 101, first two coins should always selected (100, 4).
        let expected = vec![100, 4];
        let actual: Vec<_> = results.drain(..2).collect();
        assert_eq!(expected, actual);

        // The number of dust coins is selected randomly, so we might have:
        // - 0 dust coins
        // - 1 dust coin [2]
        // - 2 dust coins [2, 3]
        // Even though in majority of cases we will have 2 dust coins selected (due to
        // MAX being huge), we can't guarantee that, hence we assert against all possible cases.
        // The important fact is that neither 100 nor 4 are selected as dust coins.
        let expected_1: Vec<u64> = vec![];
        let expected_2: Vec<u64> = vec![2];
        let expected_3: Vec<u64> = vec![2, 3];
        let actual: Vec<_> = std::mem::take(&mut results);

        assert!(
            actual == expected_1 || actual == expected_2 || actual == expected_3,
            "Unexpected dust coins: {:?}",
            actual,
        );
    }

    #[test]
    fn selection_algorithm_should_bail_on_error() {
        const MAX: u16 = u16::MAX;
        const TOTAL: u64 = 101;

        let mut coins = setup_test_coins([10, 9, 8, 7]);
        let error = fuel_core_storage::Error::NotFound("S1", "S2");

        let first_2: Vec<_> = coins.drain(..2).collect();
        let last_2: Vec<_> = std::mem::take(&mut coins);

        let excluded = ExcludedKeysAsBytes::new(vec![], vec![]);

        // Inject an error into the middle of coins.
        let coins: Vec<_> = first_2
            .into_iter()
            .take(2)
            .chain(std::iter::once(Err(error)))
            .chain(last_2)
            .collect();

        let result = select_coins_to_spend(
            coins.into_iter().into_boxed(),
            std::iter::empty().into_boxed(),
            TOTAL,
            MAX,
            &excluded,
        );

        assert!(
            matches!(result, Err(error) if error == fuel_core_storage::Error::NotFound("S1", "S2"))
        );
    }
}
