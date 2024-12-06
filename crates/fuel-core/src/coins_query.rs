use crate::{
    fuel_core_graphql_api::database::ReadView,
    query::asset_query::{
        AssetQuery,
        AssetSpendTarget,
        Exclude,
    },
};
use core::mem::swap;
use fuel_core_storage::Error as StorageError;
use fuel_core_types::{
    entities::coins::{
        CoinId,
        CoinType,
    },
    fuel_types::{
        Address,
        AssetId,
        Word,
    },
};
use futures::TryStreamExt;
use rand::prelude::*;
use std::cmp::Reverse;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoinsQueryError {
    #[error("store error occurred: {0}")]
    StorageError(StorageError),
    #[error("target can't be met without exceeding the {max} coin limit.")]
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
}

#[cfg(test)]
impl PartialEq for CoinsQueryError {
    fn eq(&self, other: &Self) -> bool {
        format!("{self:?}") == format!("{other:?}")
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
