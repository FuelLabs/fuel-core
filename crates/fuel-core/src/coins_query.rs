use crate::{
    fuel_core_graphql_api::database::ReadView,
    graphql_api::{
        ports::CoinsToSpendIndexIter,
        storage::coins::{
            CoinsToSpendIndexKey,
            IndexedCoinType,
        },
    },
    query::asset_query::{
        AssetQuery,
        AssetSpendTarget,
        Exclude,
    },
};
use core::mem::swap;
use fuel_core_services::yield_stream::StreamYieldExt;
use fuel_core_storage::{
    Error as StorageError,
    Result as StorageResult,
};
use fuel_core_types::{
    entities::coins::{
        CoinId,
        CoinType,
    },
    fuel_tx::{
        TxId,
        UtxoId,
    },
    fuel_types::{
        Address,
        AssetId,
        Nonce,
        Word,
    },
};
use futures::{
    Stream,
    StreamExt,
    TryStreamExt,
};
use rand::prelude::*;
use std::{
    cmp::Reverse,
    collections::HashSet,
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoinsQueryError {
    #[error("store error occurred: {0}")]
    StorageError(StorageError),
    #[error("the target cannot be met due to no coins available or exceeding the {max} coin limit.")]
    InsufficientCoinsForTheMax {
        asset_id: AssetId,
        collected_amount: Word,
        max: u16,
    },
    #[error("the query contains duplicate assets")]
    DuplicateAssets(AssetId),
    #[error(
        "too many excluded ids: provided ({provided}) is > than allowed ({allowed})"
    )]
    TooManyExcludedId { provided: usize, allowed: u16 },
    #[error("the query requires more coins than the max allowed coins: required ({required}) > max ({max})")]
    TooManyCoinsSelected { required: usize, max: u16 },
    #[error("incorrect coin key found in coins to spend index")]
    IncorrectCoinKeyInIndex,
    #[error("incorrect message key found in messages to spend index")]
    IncorrectMessageKeyInIndex,
    #[error("error while processing the query: {0}")]
    UnexpectedInternalState(&'static str),
    #[error("both total and max must be greater than 0 (provided total: {provided_total}, provided max: {provided_max})")]
    IncorrectQueryParameters {
        provided_total: u64,
        provided_max: u16,
    },
}

#[cfg(test)]
impl PartialEq for CoinsQueryError {
    fn eq(&self, other: &Self) -> bool {
        format!("{self:?}") == format!("{other:?}")
    }
}

pub(crate) type CoinsToSpendIndexEntry = (CoinsToSpendIndexKey, IndexedCoinType);

pub struct ExcludedCoinIds<'a> {
    coins: HashSet<&'a UtxoId>,
    messages: HashSet<&'a Nonce>,
}

impl<'a> ExcludedCoinIds<'a> {
    pub(crate) fn new(
        coins: impl Iterator<Item = &'a UtxoId>,
        messages: impl Iterator<Item = &'a Nonce>,
    ) -> Self {
        Self {
            coins: coins.collect(),
            messages: messages.collect(),
        }
    }

    pub(crate) fn is_coin_excluded(&self, coin: &UtxoId) -> bool {
        self.coins.contains(&coin)
    }

    pub(crate) fn is_message_excluded(&self, message: &Nonce) -> bool {
        self.messages.contains(&message)
    }
}

/// The prepared spend queries.
pub struct SpendQuery {
    owner: Address,
    query_per_asset: Vec<AssetSpendTarget>,
    exclude: Exclude,
    base_asset_id: AssetId,
}

impl SpendQuery {
    // TODO: Check that number of `queries` is not too high(to prevent attacks).
    //  https://github.com/FuelLabs/fuel-core/issues/588#issuecomment-1240074551
    pub fn new(
        owner: Address,
        query_per_asset: &[AssetSpendTarget],
        exclude_vec: Option<Vec<CoinId>>,
        base_asset_id: AssetId,
    ) -> Result<Self, CoinsQueryError> {
        let exclude = exclude_vec.map_or_else(Default::default, Exclude::new);

        Ok(Self {
            owner,
            query_per_asset: query_per_asset.to_vec(),
            exclude,
            base_asset_id,
        })
    }

    /// Return `Asset`s.
    pub fn assets(&self) -> &Vec<AssetSpendTarget> {
        &self.query_per_asset
    }

    /// Return [`AssetQuery`]s.
    pub fn asset_queries<'a>(&'a self, db: &'a ReadView) -> Vec<AssetQuery<'a>> {
        self.query_per_asset
            .iter()
            .map(|asset| {
                AssetQuery::new(
                    &self.owner,
                    asset,
                    &self.base_asset_id,
                    Some(&self.exclude),
                    db,
                )
            })
            .collect()
    }

    /// Returns exclude that contains information about excluded ids.
    pub fn exclude(&self) -> &Exclude {
        &self.exclude
    }

    /// Returns the owner of the query.
    pub fn owner(&self) -> &Address {
        &self.owner
    }
}

/// Returns the biggest inputs of the `owner` to satisfy the required `target` of the asset. The
/// number of inputs for each asset can't exceed `max_inputs`, otherwise throw an error that query
/// can't be satisfied.
pub async fn largest_first(
    query: AssetQuery<'_>,
) -> Result<Vec<CoinType>, CoinsQueryError> {
    let target = query.asset.target;
    let max = query.asset.max;
    let asset_id = query.asset.id;
    let mut inputs: Vec<CoinType> = query.coins().try_collect().await?;
    inputs.sort_by_key(|coin| Reverse(coin.amount()));

    let mut collected_amount = 0u64;
    let mut coins = vec![];

    for coin in inputs {
        // Break if we don't need any more coins
        if collected_amount >= target {
            break
        }

        // Error if we can't fit more coins
        if coins.len() >= max as usize {
            return Err(CoinsQueryError::InsufficientCoinsForTheMax {
                asset_id,
                collected_amount,
                max,
            })
        }

        // Add to list
        collected_amount = collected_amount.saturating_add(coin.amount());
        coins.push(coin);
    }

    if collected_amount < target {
        return Err(CoinsQueryError::InsufficientCoinsForTheMax {
            asset_id,
            collected_amount,
            max,
        })
    }

    Ok(coins)
}

// An implementation of the method described on: https://iohk.io/en/blog/posts/2018/07/03/self-organisation-in-coin-selection/
// TODO: Reimplement this algorithm to be simpler and faster:
//  Instead of selecting random coins first, we can sort them.
//  After that, we can split the coins into the part that covers the
//  target and the part that does not(by choosing the most expensive coins).
//  When the target is satisfied, we can select random coins from the remaining
//  coins not used in the target.
//  https://github.com/FuelLabs/fuel-core/issues/1965
pub async fn random_improve(
    db: &ReadView,
    spend_query: &SpendQuery,
) -> Result<Vec<Vec<CoinType>>, CoinsQueryError> {
    let mut coins_per_asset = vec![];

    for query in spend_query.asset_queries(db) {
        let target = query.asset.target;
        let max = query.asset.max;

        let mut inputs: Vec<_> = query.clone().coins().try_collect().await?;
        inputs.shuffle(&mut thread_rng());
        inputs.truncate(max as usize);

        let mut collected_amount = 0;
        let mut coins = vec![];

        // Set parameters according to spec
        let upper_target = target.saturating_mul(2);

        for coin in inputs {
            // Try to improve the result by adding dust to the result.
            if collected_amount >= target {
                // Break if found coin exceeds max `u64` or the upper limit
                if collected_amount == u64::MAX || coin.amount() > upper_target {
                    break
                }

                // Break if adding doesn't improve the distance
                let change_amount = collected_amount
                    .checked_sub(target)
                    .expect("We checked it above");
                let distance = target.abs_diff(change_amount);
                let next_distance =
                    target.abs_diff(change_amount.saturating_add(coin.amount()));
                if next_distance >= distance {
                    break
                }
            }

            // Add to list
            collected_amount = collected_amount.saturating_add(coin.amount());
            coins.push(coin);
        }

        // Fallback to largest_first if we can't fit more coins
        if collected_amount < target {
            swap(&mut coins, &mut largest_first(query).await?);
        }

        coins_per_asset.push(coins);
    }

    Ok(coins_per_asset)
}

pub async fn select_coins_to_spend(
    CoinsToSpendIndexIter {
        big_coins_iter,
        dust_coins_iter,
    }: CoinsToSpendIndexIter<'_>,
    total: u64,
    max: u16,
    excluded_ids: &ExcludedCoinIds<'_>,
    batch_size: usize,
) -> Result<Option<Vec<CoinsToSpendIndexEntry>>, CoinsQueryError> {
    const TOTAL_AMOUNT_ADJUSTMENT_FACTOR: u64 = 2;
    if total == 0 || max == 0 {
        return Err(CoinsQueryError::IncorrectQueryParameters {
            provided_total: total,
            provided_max: max,
        });
    }

    let adjusted_total = total.saturating_mul(TOTAL_AMOUNT_ADJUSTMENT_FACTOR);

    let big_coins_stream = futures::stream::iter(big_coins_iter).yield_each(batch_size);
    let dust_coins_stream = futures::stream::iter(dust_coins_iter).yield_each(batch_size);

    let (selected_big_coins_total, selected_big_coins) =
        big_coins(big_coins_stream, adjusted_total, max, excluded_ids).await?;

    if selected_big_coins_total < total {
        return Ok(None);
    }
    let Some(last_selected_big_coin) = selected_big_coins.last() else {
        // Should never happen, because at this stage we know that:
        // 1) selected_big_coins_total >= total
        // 2) total > 0
        // hence: selected_big_coins_total > 0
        // therefore, at least one coin is selected - if not, it's a bug
        return Err(CoinsQueryError::UnexpectedInternalState(
            "at least one coin should be selected",
        ));
    };

    let selected_big_coins_len = selected_big_coins.len();
    let number_of_big_coins: u16 = selected_big_coins_len.try_into().map_err(|_| {
        CoinsQueryError::TooManyCoinsSelected {
            required: selected_big_coins_len,
            max: u16::MAX,
        }
    })?;

    let max_dust_count = max_dust_count(max, number_of_big_coins);
    let (dust_coins_total, selected_dust_coins) = dust_coins(
        dust_coins_stream,
        last_selected_big_coin,
        max_dust_count,
        excluded_ids,
    )
    .await?;
    let retained_big_coins_iter =
        skip_big_coins_up_to_amount(selected_big_coins, dust_coins_total);

    Ok(Some(
        (retained_big_coins_iter.chain(selected_dust_coins)).collect(),
    ))
}

async fn big_coins(
    big_coins_stream: impl Stream<Item = StorageResult<CoinsToSpendIndexEntry>> + Unpin,
    total: u64,
    max: u16,
    excluded_ids: &ExcludedCoinIds<'_>,
) -> Result<(u64, Vec<CoinsToSpendIndexEntry>), CoinsQueryError> {
    select_coins_until(big_coins_stream, max, excluded_ids, |_, total_so_far| {
        total_so_far >= total
    })
    .await
}

async fn dust_coins(
    dust_coins_stream: impl Stream<Item = StorageResult<CoinsToSpendIndexEntry>> + Unpin,
    last_big_coin: &CoinsToSpendIndexEntry,
    max_dust_count: u16,
    excluded_ids: &ExcludedCoinIds<'_>,
) -> Result<(u64, Vec<CoinsToSpendIndexEntry>), CoinsQueryError> {
    select_coins_until(
        dust_coins_stream,
        max_dust_count,
        excluded_ids,
        |coin, _| coin == last_big_coin,
    )
    .await
}

async fn select_coins_until<F>(
    mut coins_stream: impl Stream<Item = StorageResult<CoinsToSpendIndexEntry>> + Unpin,
    max: u16,
    excluded_ids: &ExcludedCoinIds<'_>,
    predicate: F,
) -> Result<(u64, Vec<CoinsToSpendIndexEntry>), CoinsQueryError>
where
    F: Fn(&CoinsToSpendIndexEntry, u64) -> bool,
{
    let mut coins_total_value: u64 = 0;
    let mut coins = Vec::with_capacity(max as usize);
    while let Some(coin) = coins_stream.next().await {
        let coin = coin?;
        if !is_excluded(&coin, excluded_ids)? {
            if coins.len() >= max as usize || predicate(&coin, coins_total_value) {
                break;
            }
            let amount = coin.0.amount();
            coins_total_value = coins_total_value.saturating_add(amount);
            coins.push(coin);
        }
    }
    Ok((coins_total_value, coins))
}

fn is_excluded(
    (key, coin_type): &CoinsToSpendIndexEntry,
    excluded_ids: &ExcludedCoinIds,
) -> Result<bool, CoinsQueryError> {
    match coin_type {
        IndexedCoinType::Coin => {
            let utxo_id_bytes = key.foreign_key_bytes();
            let tx_id: TxId = utxo_id_bytes
                .get(..32)
                .ok_or(CoinsQueryError::IncorrectCoinKeyInIndex)?
                .try_into()
                .map_err(|_| CoinsQueryError::IncorrectCoinKeyInIndex)?;

            let output_index = u16::from_be_bytes(
                utxo_id_bytes
                    .get(32..34)
                    .ok_or(CoinsQueryError::IncorrectCoinKeyInIndex)?
                    .try_into()
                    .map_err(|_| CoinsQueryError::IncorrectCoinKeyInIndex)?,
            );
            Ok(excluded_ids.is_coin_excluded(&UtxoId::new(tx_id, output_index)))
        }
        IndexedCoinType::Message => {
            let nonce_bytes = key.foreign_key_bytes();
            let nonce: Nonce = nonce_bytes
                .get(..)
                .ok_or(CoinsQueryError::IncorrectMessageKeyInIndex)?
                .try_into()
                .map_err(|_| CoinsQueryError::IncorrectMessageKeyInIndex)?;
            Ok(excluded_ids.is_message_excluded(&nonce))
        }
    }
}

fn max_dust_count(max: u16, big_coins_len: u16) -> u16 {
    let mut rng = rand::thread_rng();
    rng.gen_range(0..=max.saturating_sub(big_coins_len))
}

fn skip_big_coins_up_to_amount(
    big_coins: impl IntoIterator<Item = CoinsToSpendIndexEntry>,
    skipped_amount: u64,
) -> impl Iterator<Item = CoinsToSpendIndexEntry> {
    let mut current_dust_coins_value = skipped_amount;
    big_coins.into_iter().skip_while(move |item| {
        let item_amount = item.0.amount();
        current_dust_coins_value
            .checked_sub(item_amount)
            .map(|new_value| {
                current_dust_coins_value = new_value;
                true
            })
            .unwrap_or(false)
    })
}

impl From<StorageError> for CoinsQueryError {
    fn from(e: StorageError) -> Self {
        CoinsQueryError::StorageError(e)
    }
}

#[allow(clippy::arithmetic_side_effects)]
#[cfg(test)]
mod tests {
    use crate::{
        coins_query::{
            largest_first,
            random_improve,
            CoinsQueryError,
            SpendQuery,
        },
        combined_database::CombinedDatabase,
        fuel_core_graphql_api::{
            api_service::ReadDatabase as ServiceDatabase,
            storage::{
                coins::{
                    owner_coin_id_key,
                    OwnedCoins,
                },
                messages::{
                    OwnedMessageIds,
                    OwnedMessageKey,
                },
            },
        },
        query::asset_query::{
            AssetQuery,
            AssetSpendTarget,
        },
    };
    use assert_matches::assert_matches;
    use fuel_core_storage::{
        iter::IterDirection,
        tables::{
            Coins,
            Messages,
        },
        StorageMutate,
    };
    use fuel_core_types::{
        blockchain::primitives::DaBlockHeight,
        entities::{
            coins::coin::{
                Coin,
                CompressedCoin,
            },
            relayer::message::{
                Message,
                MessageV1,
            },
        },
        fuel_asm::Word,
        fuel_tx::*,
    };
    use futures::TryStreamExt;
    use itertools::Itertools;
    use rand::{
        rngs::StdRng,
        Rng,
        SeedableRng,
    };
    use std::cmp::Reverse;

    fn setup_coins() -> (Address, [AssetId; 2], AssetId, TestDatabase) {
        let mut rng = StdRng::seed_from_u64(0xf00df00d);
        let owner = Address::default();
        let asset_ids = [rng.gen(), rng.gen()];
        let base_asset_id = rng.gen();
        let mut db = TestDatabase::new();
        (0..5usize).for_each(|i| {
            db.make_coin(owner, (i + 1) as Word, asset_ids[0]);
            db.make_coin(owner, (i + 1) as Word, asset_ids[1]);
        });

        (owner, asset_ids, base_asset_id, db)
    }

    fn setup_messages() -> (Address, AssetId, TestDatabase) {
        let mut rng = StdRng::seed_from_u64(0xf00df00d);
        let owner = Address::default();
        let base_asset_id = rng.gen();
        let mut db = TestDatabase::new();
        (0..5usize).for_each(|i| {
            db.make_message(owner, (i + 1) as Word);
        });

        (owner, base_asset_id, db)
    }

    fn setup_coins_and_messages() -> (Address, [AssetId; 2], AssetId, TestDatabase) {
        let mut rng = StdRng::seed_from_u64(0xf00df00d);
        let owner = Address::default();
        let base_asset_id = rng.gen();
        let asset_ids = [base_asset_id, rng.gen()];
        let mut db = TestDatabase::new();
        // 2 coins and 3 messages
        (0..2usize).for_each(|i| {
            db.make_coin(owner, (i + 1) as Word, asset_ids[0]);
        });
        (2..5usize).for_each(|i| {
            db.make_message(owner, (i + 1) as Word);
        });

        (0..5usize).for_each(|i| {
            db.make_coin(owner, (i + 1) as Word, asset_ids[1]);
        });

        (owner, asset_ids, base_asset_id, db)
    }

    mod largest_first {
        use super::*;

        async fn query(
            spend_query: &[AssetSpendTarget],
            owner: &Address,
            base_asset_id: &AssetId,
            db: &ServiceDatabase,
        ) -> Result<Vec<Vec<(AssetId, Word)>>, CoinsQueryError> {
            let mut results = vec![];

            for asset in spend_query {
                let coins = largest_first(AssetQuery::new(
                    owner,
                    asset,
                    base_asset_id,
                    None,
                    &db.test_view(),
                ))
                .await
                .map(|coins| {
                    coins
                        .iter()
                        .map(|coin| (*coin.asset_id(base_asset_id), coin.amount()))
                        .collect()
                })?;
                results.push(coins);
            }

            Ok(results)
        }

        async fn single_asset_assert(
            owner: Address,
            asset_ids: &[AssetId],
            base_asset_id: &AssetId,
            db: TestDatabase,
        ) {
            let asset_id = asset_ids[0];

            // Query some targets, including higher than the owner's balance
            for target in 0..20 {
                let coins = query(
                    &[AssetSpendTarget::new(asset_id, target, u16::MAX)],
                    &owner,
                    base_asset_id,
                    &db.service_database(),
                )
                .await;

                // Transform result for convenience
                let coins = coins.map(|coins| {
                    coins[0]
                        .iter()
                        .map(|(id, amount)| {
                            // Check the asset ID before we drop it
                            assert_eq!(id, &asset_id);

                            *amount
                        })
                        .collect::<Vec<u64>>()
                });

                match target {
                    // This should return nothing
                    0 => {
                        assert_matches!(coins, Ok(coins) if coins.is_empty())
                    }
                    // This range should return the largest coins
                    1..=5 => {
                        assert_matches!(coins, Ok(coins) if coins == vec![5])
                    }
                    // This range should return the largest two coins
                    6..=9 => {
                        assert_matches!(coins, Ok(coins) if coins == vec![5, 4])
                    }
                    // This range should return the largest three coins
                    10..=12 => {
                        assert_matches!(coins, Ok(coins) if coins == vec![5, 4, 3])
                    }
                    // This range should return the largest four coins
                    13..=14 => {
                        assert_matches!(coins, Ok(coins) if coins == vec![5, 4, 3, 2])
                    }
                    // This range should return all coins
                    15 => {
                        assert_matches!(coins, Ok(coins) if coins == vec![5, 4, 3, 2, 1])
                    }
                    // Asking for more than the owner's balance should error
                    _ => {
                        assert_matches!(
                            coins,
                            Err(CoinsQueryError::InsufficientCoinsForTheMax {
                                asset_id: _,
                                collected_amount: 15,
                                max: u16::MAX
                            })
                        )
                    }
                };
            }

            // Query with too small max_inputs
            let coins = query(
                &[AssetSpendTarget::new(asset_id, 6, 1)],
                &owner,
                base_asset_id,
                &db.service_database(),
            )
            .await;
            assert_matches!(
                coins,
                Err(CoinsQueryError::InsufficientCoinsForTheMax { .. })
            );
        }

        #[tokio::test]
        async fn single_asset_coins() {
            // Setup for coins
            let (owner, asset_ids, base_asset_id, db) = setup_coins();
            single_asset_assert(owner, &asset_ids, &base_asset_id, db).await;
        }

        #[tokio::test]
        async fn single_asset_messages() {
            // Setup for messages
            let (owner, base_asset_id, db) = setup_messages();
            single_asset_assert(owner, &[base_asset_id], &base_asset_id, db).await;
        }

        #[tokio::test]
        async fn single_asset_coins_and_messages() {
            // Setup for coins and messages
            let (owner, asset_ids, base_asset_id, db) = setup_coins_and_messages();
            single_asset_assert(owner, &asset_ids, &base_asset_id, db).await;
        }

        async fn multiple_assets_helper(
            owner: Address,
            asset_ids: &[AssetId],
            base_asset_id: &AssetId,
            db: TestDatabase,
        ) {
            let coins = query(
                &[
                    AssetSpendTarget::new(asset_ids[0], 3, u16::MAX),
                    AssetSpendTarget::new(asset_ids[1], 6, u16::MAX),
                ],
                &owner,
                base_asset_id,
                &db.service_database(),
            )
            .await;
            let expected = vec![
                vec![(asset_ids[0], 5)],
                vec![(asset_ids[1], 5), (asset_ids[1], 4)],
            ];
            assert_matches!(coins, Ok(coins) if coins == expected);
        }

        #[tokio::test]
        async fn multiple_assets_coins() {
            // Setup coins
            let (owner, asset_ids, base_asset_id, db) = setup_coins();
            multiple_assets_helper(owner, &asset_ids, &base_asset_id, db).await;
        }

        #[tokio::test]
        async fn multiple_assets_coins_and_messages() {
            // Setup coins and messages
            let (owner, asset_ids, base_asset_id, db) = setup_coins_and_messages();
            multiple_assets_helper(owner, &asset_ids, &base_asset_id, db).await;
        }
    }

    mod random_improve {
        use super::*;

        async fn query(
            query_per_asset: Vec<AssetSpendTarget>,
            owner: Address,
            asset_ids: &[AssetId],
            base_asset_id: AssetId,
            db: &ServiceDatabase,
        ) -> Result<Vec<(AssetId, u64)>, CoinsQueryError> {
            let coins = random_improve(
                &db.test_view(),
                &SpendQuery::new(owner, &query_per_asset, None, base_asset_id)?,
            )
            .await;

            // Transform result for convenience
            coins.map(|coins| {
                coins
                    .into_iter()
                    .flat_map(|coins| {
                        coins
                            .into_iter()
                            .map(|coin| (*coin.asset_id(&base_asset_id), coin.amount()))
                            .sorted_by_key(|(asset_id, amount)| {
                                (
                                    asset_ids.iter().position(|c| c == asset_id).unwrap(),
                                    Reverse(*amount),
                                )
                            })
                    })
                    .collect()
            })
        }

        async fn single_asset_assert(
            owner: Address,
            asset_ids: &[AssetId],
            base_asset_id: AssetId,
            db: TestDatabase,
        ) {
            let asset_id = asset_ids[0];

            // Query some amounts, including higher than the owner's balance
            for amount in 0..20 {
                let coins = query(
                    vec![AssetSpendTarget::new(asset_id, amount, u16::MAX)],
                    owner,
                    asset_ids,
                    base_asset_id,
                    &db.service_database(),
                )
                .await;

                // Transform result for convenience
                let coins = coins.map(|coins| {
                    coins
                        .into_iter()
                        .map(|(id, amount)| {
                            // Check the asset ID before we drop it
                            assert_eq!(id, asset_id);

                            amount
                        })
                        .collect::<Vec<u64>>()
                });

                match amount {
                    // This should return nothing
                    0 => assert_matches!(coins, Ok(coins) if coins.is_empty()),
                    // This range should...
                    1..=7 => {
                        // ...satisfy the amount
                        assert_matches!(coins, Ok(coins) if coins.iter().sum::<u64>() >= amount)
                        // ...and add more for dust management
                        // TODO: Implement the test
                    }
                    // This range should return all coins
                    8..=15 => {
                        assert_matches!(coins, Ok(coins) if coins == vec![5, 4, 3, 2, 1])
                    }
                    // Asking for more than the owner's balance should error
                    _ => {
                        assert_matches!(
                            coins,
                            Err(CoinsQueryError::InsufficientCoinsForTheMax {
                                asset_id: _,
                                collected_amount: 15,
                                max: u16::MAX
                            })
                        )
                    }
                };
            }

            // Query with too small max_inputs
            let coins = query(
                vec![AssetSpendTarget::new(
                    asset_id, 6, // target
                    1, // max
                )],
                owner,
                asset_ids,
                base_asset_id,
                &db.service_database(),
            )
            .await;
            assert_matches!(
                coins,
                Err(CoinsQueryError::InsufficientCoinsForTheMax { .. })
            );
        }

        #[tokio::test]
        async fn single_asset_coins() {
            // Setup for coins
            let (owner, asset_ids, base_asset_id, db) = setup_coins();
            single_asset_assert(owner, &asset_ids, base_asset_id, db).await;
        }

        #[tokio::test]
        async fn single_asset_messages() {
            // Setup for messages
            let (owner, base_asset_id, db) = setup_messages();
            single_asset_assert(owner, &[base_asset_id], base_asset_id, db).await;
        }

        #[tokio::test]
        async fn single_asset_coins_and_messages() {
            // Setup for coins and messages
            let (owner, asset_ids, base_asset_id, db) = setup_coins_and_messages();
            single_asset_assert(owner, &asset_ids, base_asset_id, db).await;
        }

        async fn multiple_assets_assert(
            owner: Address,
            asset_ids: &[AssetId],
            base_asset_id: AssetId,
            db: TestDatabase,
        ) {
            // Query multiple asset IDs
            let coins = query(
                vec![
                    AssetSpendTarget::new(
                        asset_ids[0],
                        3, // target
                        3, // max
                    ),
                    AssetSpendTarget::new(
                        asset_ids[1],
                        6, // target
                        3, // max
                    ),
                ],
                owner,
                asset_ids,
                base_asset_id,
                &db.service_database(),
            )
            .await;
            assert_matches!(coins, Ok(ref coins) if coins.len() <= 6);
            let coins = coins.unwrap();
            assert!(
                coins
                    .iter()
                    .filter(|c| c.0 == asset_ids[0])
                    .map(|c| c.1)
                    .sum::<u64>()
                    >= 3
            );
            assert!(
                coins
                    .iter()
                    .filter(|c| c.0 == asset_ids[1])
                    .map(|c| c.1)
                    .sum::<u64>()
                    >= 6
            );
        }

        #[tokio::test]
        async fn multiple_assets_coins() {
            // Setup coins
            let (owner, asset_ids, base_asset_id, db) = setup_coins();
            multiple_assets_assert(owner, &asset_ids, base_asset_id, db).await;
        }

        #[tokio::test]
        async fn multiple_assets_coins_and_messages() {
            // Setup coins and messages
            let (owner, asset_ids, base_asset_id, db) = setup_coins_and_messages();
            multiple_assets_assert(owner, &asset_ids, base_asset_id, db).await;
        }
    }

    mod exclusion {
        use super::*;
        use fuel_core_types::entities::coins::CoinId;

        async fn query(
            db: &ServiceDatabase,
            owner: Address,
            base_asset_id: AssetId,
            asset_ids: &[AssetId],
            query_per_asset: Vec<AssetSpendTarget>,
            excluded_ids: Vec<CoinId>,
        ) -> Result<Vec<(AssetId, u64)>, CoinsQueryError> {
            let spend_query = SpendQuery::new(
                owner,
                &query_per_asset,
                Some(excluded_ids),
                base_asset_id,
            )?;
            let coins = random_improve(&db.test_view(), &spend_query).await;

            // Transform result for convenience
            coins.map(|coins| {
                coins
                    .into_iter()
                    .flat_map(|coin| {
                        coin.into_iter()
                            .map(|coin| (*coin.asset_id(&base_asset_id), coin.amount()))
                            .sorted_by_key(|(asset_id, amount)| {
                                (
                                    asset_ids.iter().position(|c| c == asset_id).unwrap(),
                                    Reverse(*amount),
                                )
                            })
                    })
                    .collect()
            })
        }

        async fn exclusion_assert(
            owner: Address,
            asset_ids: &[AssetId],
            base_asset_id: AssetId,
            db: TestDatabase,
            excluded_ids: Vec<CoinId>,
        ) {
            let asset_id = asset_ids[0];

            // Query some amounts, including higher than the owner's balance
            for amount in 0..20 {
                let coins = query(
                    &db.service_database(),
                    owner,
                    base_asset_id,
                    asset_ids,
                    vec![AssetSpendTarget::new(asset_id, amount, u16::MAX)],
                    excluded_ids.clone(),
                )
                .await;

                // Transform result for convenience
                let coins = coins.map(|coins| {
                    coins
                        .into_iter()
                        .map(|(id, amount)| {
                            // Check the asset ID before we drop it
                            assert_eq!(id, asset_id);
                            amount
                        })
                        .collect::<Vec<u64>>()
                });

                match amount {
                    // This should return nothing
                    0 => assert_matches!(coins, Ok(coins) if coins.is_empty()),
                    // This range should...
                    1..=4 => {
                        // ...satisfy the amount
                        assert_matches!(coins, Ok(coins) if coins.iter().sum::<u64>() >= amount)
                        // ...and add more for dust management
                        // TODO: Implement the test
                    }
                    // This range should return all coins
                    5..=10 => {
                        assert_matches!(coins, Ok(coins) if coins == vec![4, 3, 2, 1])
                    }
                    // Asking for more than the owner's balance should error
                    _ => {
                        assert_matches!(
                            coins,
                            Err(CoinsQueryError::InsufficientCoinsForTheMax {
                                asset_id: _,
                                collected_amount: 10,
                                max: u16::MAX
                            })
                        )
                    }
                };
            }
        }

        #[tokio::test]
        async fn exclusion_coins() {
            // Setup coins
            let (owner, asset_ids, base_asset_id, db) = setup_coins();

            // Exclude largest coin IDs
            let excluded_ids = db
                .owned_coins(&owner)
                .await
                .into_iter()
                .filter(|coin| coin.amount == 5)
                .map(|coin| CoinId::Utxo(coin.utxo_id))
                .collect_vec();

            exclusion_assert(owner, &asset_ids, base_asset_id, db, excluded_ids).await;
        }

        #[tokio::test]
        async fn exclusion_messages() {
            // Setup messages
            let (owner, base_asset_id, db) = setup_messages();

            // Exclude largest messages IDs
            let excluded_ids = db
                .owned_messages(&owner)
                .await
                .into_iter()
                .filter(|message| message.amount() == 5)
                .map(|message| CoinId::Message(*message.id()))
                .collect_vec();

            exclusion_assert(owner, &[base_asset_id], base_asset_id, db, excluded_ids)
                .await;
        }

        #[tokio::test]
        async fn exclusion_coins_and_messages() {
            // Setup coins and messages
            let (owner, asset_ids, base_asset_id, db) = setup_coins_and_messages();

            // Exclude largest messages IDs, because coins only 1 and 2
            let excluded_ids = db
                .owned_messages(&owner)
                .await
                .into_iter()
                .filter(|message| message.amount() == 5)
                .map(|message| CoinId::Message(*message.id()))
                .collect_vec();

            exclusion_assert(owner, &asset_ids, base_asset_id, db, excluded_ids).await;
        }
    }

    mod indexed_coins_to_spend {
        use fuel_core_storage::iter::IntoBoxedIter;
        use fuel_core_types::{
            entities::coins::coin::Coin,
            fuel_tx::{
                TxId,
                UtxoId,
            },
        };

        use crate::{
            coins_query::{
                select_coins_to_spend,
                select_coins_until,
                CoinsQueryError,
                CoinsToSpendIndexEntry,
                ExcludedCoinIds,
            },
            graphql_api::{
                ports::CoinsToSpendIndexIter,
                storage::coins::{
                    CoinsToSpendIndexKey,
                    IndexedCoinType,
                },
            },
        };

        const BATCH_SIZE: usize = 1;

        struct TestCoinSpec {
            index_entry: Result<CoinsToSpendIndexEntry, fuel_core_storage::Error>,
            utxo_id: UtxoId,
        }

        fn setup_test_coins(coins: impl IntoIterator<Item = u8>) -> Vec<TestCoinSpec> {
            coins
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

                    TestCoinSpec {
                        index_entry: Ok((
                            CoinsToSpendIndexKey::from_coin(&coin),
                            IndexedCoinType::Coin,
                        )),
                        utxo_id,
                    }
                })
                .collect()
        }

        #[tokio::test]
        async fn select_coins_until_respects_max() {
            // Given
            const MAX: u16 = 3;

            let coins = setup_test_coins([1, 2, 3, 4, 5]);
            let (coins, _): (Vec<_>, Vec<_>) = coins
                .into_iter()
                .map(|spec| (spec.index_entry, spec.utxo_id))
                .unzip();

            let excluded = ExcludedCoinIds::new(std::iter::empty(), std::iter::empty());

            // When
            let result = select_coins_until(
                futures::stream::iter(coins),
                MAX,
                &excluded,
                |_, _| false,
            )
            .await
            .expect("should select coins");

            // Then
            assert_eq!(result.0, 1 + 2 + 3); // Limit is set at 3 coins
            assert_eq!(result.1.len(), 3);
        }

        #[tokio::test]
        async fn select_coins_until_respects_excluded_ids() {
            // Given
            const MAX: u16 = u16::MAX;

            let coins = setup_test_coins([1, 2, 3, 4, 5]);
            let (coins, utxo_ids): (Vec<_>, Vec<_>) = coins
                .into_iter()
                .map(|spec| (spec.index_entry, spec.utxo_id))
                .unzip();

            // Exclude coin with amount '2'.
            let utxo_id = utxo_ids[1];
            let excluded =
                ExcludedCoinIds::new(std::iter::once(&utxo_id), std::iter::empty());

            // When
            let result = select_coins_until(
                futures::stream::iter(coins),
                MAX,
                &excluded,
                |_, _| false,
            )
            .await
            .expect("should select coins");

            // Then
            assert_eq!(result.0, 1 + 3 + 4 + 5); // '2' is skipped.
            assert_eq!(result.1.len(), 4);
        }

        #[tokio::test]
        async fn select_coins_until_respects_predicate() {
            // Given
            const MAX: u16 = u16::MAX;
            const TOTAL: u64 = 7;

            let coins = setup_test_coins([1, 2, 3, 4, 5]);
            let (coins, _): (Vec<_>, Vec<_>) = coins
                .into_iter()
                .map(|spec| (spec.index_entry, spec.utxo_id))
                .unzip();

            let excluded = ExcludedCoinIds::new(std::iter::empty(), std::iter::empty());

            let predicate: fn(&CoinsToSpendIndexEntry, u64) -> bool =
                |_, total| total > TOTAL;

            // When
            let result = select_coins_until(
                futures::stream::iter(coins),
                MAX,
                &excluded,
                predicate,
            )
            .await
            .expect("should select coins");

            // Then
            assert_eq!(result.0, 1 + 2 + 3 + 4); // Keep selecting until total is greater than 7.
            assert_eq!(result.1.len(), 4);
        }

        #[tokio::test]
        async fn already_selected_big_coins_are_never_reselected_as_dust() {
            // Given
            const MAX: u16 = u16::MAX;
            const TOTAL: u64 = 101;

            let test_coins = [100, 100, 4, 3, 2];
            let big_coins_iter = setup_test_coins(test_coins)
                .into_iter()
                .map(|spec| spec.index_entry)
                .into_boxed();

            let dust_coins_iter = setup_test_coins(test_coins)
                .into_iter()
                .rev()
                .map(|spec| spec.index_entry)
                .into_boxed();

            let coins_to_spend_iter = CoinsToSpendIndexIter {
                big_coins_iter,
                dust_coins_iter,
            };

            let excluded = ExcludedCoinIds::new(std::iter::empty(), std::iter::empty());

            // When
            let result = select_coins_to_spend(
                coins_to_spend_iter,
                TOTAL,
                MAX,
                &excluded,
                BATCH_SIZE,
            )
            .await
            .expect("should not error")
            .expect("should select some coins");

            let mut results = result
                .into_iter()
                .map(|(key, _)| key.amount())
                .collect::<Vec<_>>();

            // Then

            // Because we select a total of 202 (TOTAL * 2), first 3 coins should always selected (100, 100, 4).
            let expected = vec![100, 100, 4];
            let actual: Vec<_> = results.drain(..3).collect();
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

        #[tokio::test]
        async fn selects_double_the_value_of_coins() {
            // Given
            const MAX: u16 = u16::MAX;
            const TOTAL: u64 = 10;

            let coins = setup_test_coins([10, 10, 9, 8, 7]);
            let (coins, _): (Vec<_>, Vec<_>) = coins
                .into_iter()
                .map(|spec| (spec.index_entry, spec.utxo_id))
                .unzip();

            let excluded = ExcludedCoinIds::new(std::iter::empty(), std::iter::empty());

            let coins_to_spend_iter = CoinsToSpendIndexIter {
                big_coins_iter: coins.into_iter().into_boxed(),
                dust_coins_iter: std::iter::empty().into_boxed(),
            };

            // When
            let result = select_coins_to_spend(
                coins_to_spend_iter,
                TOTAL,
                MAX,
                &excluded,
                BATCH_SIZE,
            )
            .await
            .expect("should not error")
            .expect("should select some coins");

            // Then
            let results: Vec<_> =
                result.into_iter().map(|(key, _)| key.amount()).collect();
            assert_eq!(results, vec![10, 10]);
        }

        #[tokio::test]
        async fn selection_algorithm_should_bail_on_storage_error() {
            // Given
            const MAX: u16 = u16::MAX;
            const TOTAL: u64 = 101;

            let coins = setup_test_coins([10, 9, 8, 7]);
            let (mut coins, _): (Vec<_>, Vec<_>) = coins
                .into_iter()
                .map(|spec| (spec.index_entry, spec.utxo_id))
                .unzip();
            let error = fuel_core_storage::Error::NotFound("S1", "S2");

            let first_2: Vec<_> = coins.drain(..2).collect();
            let last_2: Vec<_> = std::mem::take(&mut coins);

            let excluded = ExcludedCoinIds::new(std::iter::empty(), std::iter::empty());

            // Inject an error into the middle of coins.
            let coins: Vec<_> = first_2
                .into_iter()
                .take(2)
                .chain(std::iter::once(Err(error)))
                .chain(last_2)
                .collect();
            let coins_to_spend_iter = CoinsToSpendIndexIter {
                big_coins_iter: coins.into_iter().into_boxed(),
                dust_coins_iter: std::iter::empty().into_boxed(),
            };

            // When
            let result = select_coins_to_spend(
                coins_to_spend_iter,
                TOTAL,
                MAX,
                &excluded,
                BATCH_SIZE,
            )
            .await;

            // Then
            assert!(matches!(result, Err(actual_error)
                if CoinsQueryError::StorageError(fuel_core_storage::Error::NotFound("S1", "S2")) == actual_error));
        }

        #[tokio::test]
        async fn selection_algorithm_should_bail_on_incorrect_max() {
            // Given
            const MAX: u16 = 0;
            const TOTAL: u64 = 101;

            let excluded = ExcludedCoinIds::new(std::iter::empty(), std::iter::empty());

            let coins_to_spend_iter = CoinsToSpendIndexIter {
                big_coins_iter: std::iter::empty().into_boxed(),
                dust_coins_iter: std::iter::empty().into_boxed(),
            };

            let result = select_coins_to_spend(
                coins_to_spend_iter,
                TOTAL,
                MAX,
                &excluded,
                BATCH_SIZE,
            )
            .await;

            // Then
            assert!(matches!(result, Err(actual_error)
                if CoinsQueryError::IncorrectQueryParameters{ provided_total: 101, provided_max: 0 } == actual_error));
        }

        #[tokio::test]
        async fn selection_algorithm_should_bail_on_incorrect_total() {
            // Given
            const MAX: u16 = 101;
            const TOTAL: u64 = 0;

            let excluded = ExcludedCoinIds::new(std::iter::empty(), std::iter::empty());

            let coins_to_spend_iter = CoinsToSpendIndexIter {
                big_coins_iter: std::iter::empty().into_boxed(),
                dust_coins_iter: std::iter::empty().into_boxed(),
            };

            let result = select_coins_to_spend(
                coins_to_spend_iter,
                TOTAL,
                MAX,
                &excluded,
                BATCH_SIZE,
            )
            .await;

            // Then
            assert!(matches!(result, Err(actual_error)
                if CoinsQueryError::IncorrectQueryParameters{ provided_total: 0, provided_max: 101 } == actual_error));
        }
    }

    #[derive(Clone, Debug)]
    struct TestCase {
        db_amount: Vec<Word>,
        target_amount: u64,
        max_coins: u16,
    }

    pub enum CoinType {
        Coin,
        Message,
    }

    async fn test_case_run(
        case: TestCase,
        coin_type: CoinType,
        base_asset_id: AssetId,
    ) -> Result<usize, CoinsQueryError> {
        let TestCase {
            db_amount,
            target_amount,
            max_coins,
        } = case;
        let owner = Address::default();
        let asset_ids = [base_asset_id];
        let mut db = TestDatabase::new();
        for amount in db_amount {
            match coin_type {
                CoinType::Coin => {
                    let _ = db.make_coin(owner, amount, asset_ids[0]);
                }
                CoinType::Message => {
                    let _ = db.make_message(owner, amount);
                }
            };
        }

        let coins = random_improve(
            &db.service_database().test_view(),
            &SpendQuery::new(
                owner,
                &[AssetSpendTarget {
                    id: asset_ids[0],
                    target: target_amount,
                    max: max_coins,
                }],
                None,
                base_asset_id,
            )?,
        )
        .await?;

        assert_eq!(coins.len(), 1);
        Ok(coins[0].len())
    }

    #[tokio::test]
    async fn insufficient_coins_returns_error() {
        let test_case = TestCase {
            db_amount: vec![0],
            target_amount: u64::MAX,
            max_coins: u16::MAX,
        };
        let mut rng = StdRng::seed_from_u64(0xF00DF00D);
        let base_asset_id = rng.gen();
        let coin_result =
            test_case_run(test_case.clone(), CoinType::Coin, base_asset_id).await;
        let message_result =
            test_case_run(test_case, CoinType::Message, base_asset_id).await;
        assert_eq!(coin_result, message_result);
        assert_matches!(
            coin_result,
            Err(CoinsQueryError::InsufficientCoinsForTheMax {
                asset_id: _base_asset_id,
                collected_amount: 0,
                max: u16::MAX
            })
        )
    }

    #[test_case::test_case(
        TestCase {
            db_amount: vec![u64::MAX, u64::MAX],
            target_amount: u64::MAX,
            max_coins: u16::MAX,
        }
        => Ok(1)
        ; "Enough coins in the DB to reach target(u64::MAX) by 1 coin"
    )]
    #[test_case::test_case(
        TestCase {
            db_amount: vec![2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, u64::MAX - 1],
            target_amount: u64::MAX,
            max_coins: 2,
        }
        => Ok(2)
        ; "Enough coins in the DB to reach target(u64::MAX) by 2 coins"
    )]
    #[tokio::test]
    async fn corner_cases(case: TestCase) -> Result<usize, CoinsQueryError> {
        let mut rng = StdRng::seed_from_u64(0xF00DF00D);
        let base_asset_id = rng.gen();
        let coin_result =
            test_case_run(case.clone(), CoinType::Coin, base_asset_id).await;
        let message_result = test_case_run(case, CoinType::Message, base_asset_id).await;
        assert_eq!(coin_result, message_result);
        coin_result
    }

    #[tokio::test]
    async fn enough_coins_in_the_db_to_reach_target_u64_max_but_limit_is_zero() {
        let mut rng = StdRng::seed_from_u64(0xF00DF00D);

        let case = TestCase {
            db_amount: vec![u64::MAX, u64::MAX],
            target_amount: u64::MAX,
            max_coins: 0,
        };

        let base_asset_id = rng.gen();
        let coin_result =
            test_case_run(case.clone(), CoinType::Coin, base_asset_id).await;
        let message_result = test_case_run(case, CoinType::Message, base_asset_id).await;
        assert_eq!(coin_result, message_result);
        assert!(matches!(
            coin_result,
            Err(CoinsQueryError::InsufficientCoinsForTheMax { .. })
        ));
    }

    // TODO: Should use any mock database instead of the `fuel_core::CombinedDatabase`.
    pub struct TestDatabase {
        database: CombinedDatabase,
        last_coin_index: u64,
        last_message_index: u64,
    }

    impl TestDatabase {
        fn new() -> Self {
            Self {
                database: Default::default(),
                last_coin_index: Default::default(),
                last_message_index: Default::default(),
            }
        }

        fn service_database(&self) -> ServiceDatabase {
            let on_chain = self.database.on_chain().clone();
            let off_chain = self.database.off_chain().clone();
            ServiceDatabase::new(100, 0u32.into(), on_chain, off_chain)
                .expect("should create service database")
        }
    }

    impl TestDatabase {
        pub fn make_coin(
            &mut self,
            owner: Address,
            amount: Word,
            asset_id: AssetId,
        ) -> Coin {
            let index = self.last_coin_index;
            self.last_coin_index += 1;

            let id = UtxoId::new(Bytes32::from([0u8; 32]), index.try_into().unwrap());
            let mut coin = CompressedCoin::default();
            coin.set_owner(owner);
            coin.set_amount(amount);
            coin.set_asset_id(asset_id);

            let db = self.database.on_chain_mut();
            StorageMutate::<Coins>::insert(db, &id, &coin).unwrap();
            let db = self.database.off_chain_mut();
            let coin_by_owner = owner_coin_id_key(&owner, &id);
            StorageMutate::<OwnedCoins>::insert(db, &coin_by_owner, &()).unwrap();

            coin.uncompress(id)
        }

        pub fn make_message(&mut self, owner: Address, amount: Word) -> Message {
            let nonce = self.last_message_index.into();
            self.last_message_index += 1;

            let message: Message = MessageV1 {
                sender: Default::default(),
                recipient: owner,
                nonce,
                amount,
                data: vec![],
                da_height: DaBlockHeight::from(1u64),
            }
            .into();

            let db = self.database.on_chain_mut();
            StorageMutate::<Messages>::insert(db, message.id(), &message).unwrap();
            let db = self.database.off_chain_mut();
            let owned_message_key = OwnedMessageKey::new(&owner, &nonce);
            StorageMutate::<OwnedMessageIds>::insert(db, &owned_message_key, &())
                .unwrap();

            message
        }

        pub async fn owned_coins(&self, owner: &Address) -> Vec<Coin> {
            let query = self.service_database();
            let query = query.test_view();
            query
                .owned_coins(owner, None, IterDirection::Forward)
                .try_collect()
                .await
                .unwrap()
        }

        pub async fn owned_messages(&self, owner: &Address) -> Vec<Message> {
            let query = self.service_database();
            let query = query.test_view();
            query
                .owned_messages(owner, None, IterDirection::Forward)
                .try_collect()
                .await
                .unwrap()
        }
    }
}
