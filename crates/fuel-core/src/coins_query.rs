use crate::{
    fuel_core_graphql_api::service::Database,
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
use itertools::Itertools;
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
    #[error("not enough coins to fit the target")]
    InsufficientCoins {
        asset_id: AssetId,
        collected_amount: Word,
    },
    #[error("max number of coins is reached while trying to fit the target")]
    MaxCoinsReached,
    #[error("the query contains duplicate assets")]
    DuplicateAssets(AssetId),
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
}

impl SpendQuery {
    // TODO: Check that number of `queries` is not too high(to prevent attacks).
    //  https://github.com/FuelLabs/fuel-core/issues/588#issuecomment-1240074551
    pub fn new(
        owner: Address,
        query_per_asset: &[AssetSpendTarget],
        exclude_vec: Option<Vec<CoinId>>,
    ) -> Result<Self, CoinsQueryError> {
        let mut duplicate_checker = HashSet::new();

        for query in query_per_asset {
            if duplicate_checker.contains(&query.id) {
                return Err(CoinsQueryError::DuplicateAssets(query.id))
            }
            duplicate_checker.insert(query.id);
        }

        let exclude = if let Some(exclude_vec) = exclude_vec {
            Exclude::new(exclude_vec)
        } else {
            Default::default()
        };

        Ok(Self {
            owner,
            query_per_asset: query_per_asset.into(),
            exclude,
        })
    }

    /// Return [`Asset`]s.
    pub fn assets(&self) -> &Vec<AssetSpendTarget> {
        &self.query_per_asset
    }

    /// Return [`AssetQuery`]s.
    pub fn asset_queries<'a>(&'a self, db: &'a Database) -> Vec<AssetQuery<'a>> {
        self.query_per_asset
            .iter()
            .map(|asset| AssetQuery::new(&self.owner, asset, Some(&self.exclude), db))
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
pub fn largest_first(query: &AssetQuery) -> Result<Vec<CoinType>, CoinsQueryError> {
    let mut inputs: Vec<_> = query.coins().try_collect()?;
    inputs.sort_by_key(|coin| Reverse(coin.amount()));

    let mut collected_amount = 0u64;
    let mut coins = vec![];

    for coin in inputs {
        // Break if we don't need any more coins
        if collected_amount >= query.asset.target {
            break
        }

        // Error if we can't fit more coins
        if coins.len() >= query.asset.max {
            return Err(CoinsQueryError::MaxCoinsReached)
        }

        // Add to list
        collected_amount = collected_amount.saturating_add(coin.amount());
        coins.push(coin);
    }

    if collected_amount < query.asset.target {
        return Err(CoinsQueryError::InsufficientCoins {
            asset_id: query.asset.id,
            collected_amount,
        })
    }

    Ok(coins)
}

// An implementation of the method described on: https://iohk.io/en/blog/posts/2018/07/03/self-organisation-in-coin-selection/
pub fn random_improve(
    db: &Database,
    spend_query: &SpendQuery,
) -> Result<Vec<Vec<CoinType>>, CoinsQueryError> {
    let mut coins_per_asset = vec![];

    for query in spend_query.asset_queries(db) {
        let mut inputs: Vec<_> = query.coins().try_collect()?;
        inputs.shuffle(&mut thread_rng());
        inputs.truncate(query.asset.max);

        let mut collected_amount = 0;
        let mut coins = vec![];

        // Set parameters according to spec
        let target = query.asset.target;
        let upper_target = query.asset.target.saturating_mul(2);

        for coin in inputs {
            // Try to improve the result by adding dust to the result.
            if collected_amount >= target {
                // Break if found coin exceeds max `u64` or the upper limit
                if collected_amount == u64::MAX || coin.amount() > upper_target {
                    break
                }

                // Break if adding doesn't improve the distance
                let change_amount = collected_amount - target;
                let distance = target.abs_diff(change_amount);
                let next_distance = target.abs_diff(change_amount + coin.amount());
                if next_distance >= distance {
                    break
                }
            }

            // Add to list
            collected_amount = collected_amount.saturating_add(coin.amount());
            coins.push(coin);
        }

        // Fallback to largest_first if we can't fit more coins
        if collected_amount < query.asset.target {
            swap(&mut coins, &mut largest_first(&query)?);
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

#[cfg(test)]
mod tests {
    use crate::{
        coins_query::{
            largest_first,
            random_improve,
            CoinsQueryError,
            SpendQuery,
        },
        database::Database,
        fuel_core_graphql_api::service::Database as ServiceDatabase,
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
            message::Message,
        },
        fuel_asm::Word,
        fuel_tx::*,
    };
    use itertools::Itertools;
    use std::cmp::Reverse;

    fn setup_coins() -> (Address, [AssetId; 2], TestDatabase) {
        let owner = Address::default();
        let asset_ids = [AssetId::new([1u8; 32]), AssetId::new([2u8; 32])];
        let mut db = TestDatabase::new();
        (0..5usize).for_each(|i| {
            db.make_coin(owner, (i + 1) as Word, asset_ids[0]);
            db.make_coin(owner, (i + 1) as Word, asset_ids[1]);
        });

        (owner, asset_ids, db)
    }

    fn setup_messages() -> (Address, AssetId, TestDatabase) {
        let owner = Address::default();
        let asset_id = AssetId::BASE;
        let mut db = TestDatabase::new();
        (0..5usize).for_each(|i| {
            db.make_message(owner, (i + 1) as Word);
        });

        (owner, asset_id, db)
    }

    fn setup_coins_and_messages() -> (Address, [AssetId; 2], TestDatabase) {
        let owner = Address::default();
        let asset_ids = [AssetId::BASE, AssetId::new([1u8; 32])];
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

        (owner, asset_ids, db)
    }

    mod largest_first {
        use super::*;

        fn query(
            spend_query: &[AssetSpendTarget],
            owner: &Address,
            db: &ServiceDatabase,
        ) -> Result<Vec<Vec<(AssetId, Word)>>, CoinsQueryError> {
            let result: Vec<_> = spend_query
                .iter()
                .map(|asset| {
                    largest_first(&AssetQuery::new(owner, asset, None, db)).map(|coins| {
                        coins
                            .iter()
                            .map(|coin| (*coin.asset_id(), coin.amount()))
                            .collect()
                    })
                })
                .try_collect()?;
            Ok(result)
        }

        fn single_asset_assert(owner: Address, asset_ids: &[AssetId], db: TestDatabase) {
            let asset_id = asset_ids[0];

            // Query some targets, including higher than the owner's balance
            for target in 0..20 {
                let coins = query(
                    &[AssetSpendTarget::new(asset_id, target, u64::MAX)],
                    &owner,
                    &db.service_database(),
                );

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
                            Err(CoinsQueryError::InsufficientCoins {
                                asset_id: _,
                                collected_amount: 15,
                            })
                        )
                    }
                };
            }

            // Query with too small max_inputs
            let coins = query(
                &[AssetSpendTarget::new(asset_id, 6, 1)],
                &owner,
                &db.service_database(),
            );
            assert_matches!(coins, Err(CoinsQueryError::MaxCoinsReached));
        }

        #[test]
        fn single_asset() {
            // Setup for coins
            let (owner, asset_ids, db) = setup_coins();
            single_asset_assert(owner, &asset_ids, db);

            // Setup for messages
            let (owner, asset_ids, db) = setup_messages();
            single_asset_assert(owner, &[asset_ids], db);

            // Setup for coins and messages
            let (owner, asset_ids, db) = setup_coins_and_messages();
            single_asset_assert(owner, &asset_ids, db);
        }

        fn multiple_assets_helper(
            owner: Address,
            asset_ids: &[AssetId],
            db: TestDatabase,
        ) {
            let coins = query(
                &[
                    AssetSpendTarget::new(asset_ids[0], 3, u64::MAX),
                    AssetSpendTarget::new(asset_ids[1], 6, u64::MAX),
                ],
                &owner,
                &db.service_database(),
            );
            assert_matches!(coins, Ok(coins)
            if coins == vec![
                vec![(asset_ids[0], 5)],
                vec![(asset_ids[1], 5), (asset_ids[1], 4)]
            ]);
        }

        #[test]
        fn multiple_assets() {
            // Setup coins
            let (owner, asset_ids, db) = setup_coins();
            multiple_assets_helper(owner, &asset_ids, db);

            // Setup coins and messages
            let (owner, asset_ids, db) = setup_coins_and_messages();
            multiple_assets_helper(owner, &asset_ids, db);
        }
    }

    mod random_improve {
        use super::*;

        fn query(
            query_per_asset: Vec<AssetSpendTarget>,
            owner: Address,
            asset_ids: &[AssetId],
            db: &ServiceDatabase,
        ) -> Result<Vec<(AssetId, u64)>, CoinsQueryError> {
            let coins =
                random_improve(db, &SpendQuery::new(owner, &query_per_asset, None)?);

            // Transform result for convenience
            coins.map(|coins| {
                coins
                    .into_iter()
                    .flat_map(|coins| {
                        coins
                            .into_iter()
                            .map(|coin| (*coin.asset_id(), coin.amount()))
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

        fn single_asset_assert(owner: Address, asset_ids: &[AssetId], db: TestDatabase) {
            let asset_id = asset_ids[0];

            // Query some amounts, including higher than the owner's balance
            for amount in 0..20 {
                let coins = query(
                    vec![AssetSpendTarget::new(asset_id, amount, u64::MAX)],
                    owner,
                    asset_ids,
                    &db.service_database(),
                );

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
                            Err(CoinsQueryError::InsufficientCoins {
                                asset_id: _,
                                collected_amount: 15,
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
                &db.service_database(),
            );
            assert_matches!(coins, Err(CoinsQueryError::MaxCoinsReached));
        }

        #[test]
        fn single_asset() {
            // Setup for coins
            let (owner, asset_ids, db) = setup_coins();
            single_asset_assert(owner, &asset_ids, db);

            // Setup for messages
            let (owner, asset_ids, db) = setup_messages();
            single_asset_assert(owner, &[asset_ids], db);

            // Setup for coins and messages
            let (owner, asset_ids, db) = setup_coins_and_messages();
            single_asset_assert(owner, &asset_ids, db);
        }

        fn multiple_assets_assert(
            owner: Address,
            asset_ids: &[AssetId],
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
                &db.service_database(),
            );
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

        #[test]
        fn multiple_assets() {
            // Setup coins
            let (owner, asset_ids, db) = setup_coins();
            multiple_assets_assert(owner, &asset_ids, db);

            // Setup coins and messages
            let (owner, asset_ids, db) = setup_coins_and_messages();
            multiple_assets_assert(owner, &asset_ids, db);
        }
    }

    mod exclusion {
        use super::*;
        use fuel_core_types::entities::coins::CoinId;

        fn exclusion_assert(
            owner: Address,
            asset_ids: &[AssetId],
            db: TestDatabase,
            excluded_ids: Vec<CoinId>,
        ) {
            let asset_id = asset_ids[0];

            let query = |query_per_asset: Vec<AssetSpendTarget>,
                         excluded_ids: Vec<CoinId>|
             -> Result<Vec<(AssetId, u64)>, CoinsQueryError> {
                let coins = random_improve(
                    &db.service_database(),
                    &SpendQuery::new(owner, &query_per_asset, Some(excluded_ids))?,
                );

                // Transform result for convenience
                coins.map(|coins| {
                    coins
                        .into_iter()
                        .flat_map(|coin| {
                            coin.into_iter()
                                .map(|coin| (*coin.asset_id(), coin.amount()))
                                .sorted_by_key(|(asset_id, amount)| {
                                    (
                                        asset_ids
                                            .iter()
                                            .position(|c| c == asset_id)
                                            .unwrap(),
                                        Reverse(*amount),
                                    )
                                })
                        })
                        .collect()
                })
            };

            // Query some amounts, including higher than the owner's balance
            for amount in 0..20 {
                let coins = query(
                    vec![AssetSpendTarget::new(asset_id, amount, u64::MAX)],
                    excluded_ids.clone(),
                );

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
                            Err(CoinsQueryError::InsufficientCoins {
                                asset_id: _,
                                collected_amount: 10,
                            })
                        )
                    }
                };
            }
        }

        #[test]
        fn exclusion() {
            // Setup coins
            let (owner, asset_ids, db) = setup_coins();

            // Exclude largest coin IDs
            let excluded_ids = db
                .owned_coins(&owner)
                .into_iter()
                .filter(|coin| coin.amount == 5)
                .map(|coin| CoinId::Utxo(coin.utxo_id))
                .collect_vec();

            exclusion_assert(owner, &asset_ids, db, excluded_ids);

            // Setup messages
            let (owner, asset_ids, db) = setup_messages();

            // Exclude largest messages IDs
            let excluded_ids = db
                .owned_messages(&owner)
                .into_iter()
                .filter(|message| message.amount == 5)
                .map(|message| CoinId::Message(*message.id()))
                .collect_vec();

            exclusion_assert(owner, &[asset_ids], db, excluded_ids);

            // Setup coins and messages
            let (owner, asset_ids, db) = setup_coins_and_messages();

            // Exclude largest messages IDs, because coins only 1 and 2
            let excluded_ids = db
                .owned_messages(&owner)
                .into_iter()
                .filter(|message| message.amount == 5)
                .map(|message| CoinId::Message(*message.id()))
                .collect_vec();

            exclusion_assert(owner, &asset_ids, db, excluded_ids);
        }
    }

    #[derive(Clone, Debug)]
    struct TestCase {
        db_amount: Vec<Word>,
        target_amount: u64,
        max_coins: usize,
    }

    #[test_case::test_case(
        TestCase {
            db_amount: vec![0],
            target_amount: u64::MAX,
            max_coins: usize::MAX,
        }
        => Err(CoinsQueryError::InsufficientCoins {
            asset_id: AssetId::BASE, collected_amount: 0
        })
        ; "Insufficient coins in the DB(0) to reach target(u64::MAX)"
    )]
    #[test_case::test_case(
        TestCase {
            db_amount: vec![u64::MAX, u64::MAX],
            target_amount: u64::MAX,
            max_coins: usize::MAX,
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
    #[test_case::test_case(
        TestCase {
            db_amount: vec![u64::MAX, u64::MAX],
            target_amount: u64::MAX,
            max_coins: 0,
        }
        => Err(CoinsQueryError::MaxCoinsReached)
        ; "Enough coins in the DB to reach target(u64::MAX) but limit is zero"
    )]
    fn corner_cases(case: TestCase) -> Result<usize, CoinsQueryError> {
        pub enum CoinType {
            Coin,
            Message,
        }

        fn test_case_run(
            case: TestCase,
            coin_type: CoinType,
        ) -> Result<usize, CoinsQueryError> {
            let TestCase {
                db_amount,
                target_amount,
                max_coins,
            } = case;
            let owner = Address::default();
            let asset_ids = vec![AssetId::BASE];
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
                &db.service_database(),
                &SpendQuery::new(
                    owner,
                    &[AssetSpendTarget {
                        id: asset_ids[0],
                        target: target_amount,
                        max: max_coins,
                    }],
                    None,
                )?,
            )?;

            assert_eq!(coins.len(), 1);
            Ok(coins[0].len())
        }

        let coin_result = test_case_run(case.clone(), CoinType::Coin);
        let message_result = test_case_run(case, CoinType::Message);
        assert_eq!(coin_result, message_result);

        coin_result
    }

    pub struct TestDatabase {
        database: Database,
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
            Box::new(self.database.clone())
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
            let coin = CompressedCoin {
                owner,
                amount,
                asset_id,
                maturity: Default::default(),
                tx_pointer: Default::default(),
            };

            let db = &mut self.database;
            StorageMutate::<Coins>::insert(db, &id, &coin).unwrap();

            coin.uncompress(id)
        }

        pub fn make_message(&mut self, owner: Address, amount: Word) -> Message {
            let nonce = self.last_message_index.into();
            self.last_message_index += 1;

            let message = Message {
                sender: Default::default(),
                recipient: owner,
                nonce,
                amount,
                data: vec![],
                da_height: DaBlockHeight::from(1u64),
            };

            let db = &mut self.database;
            StorageMutate::<Messages>::insert(db, message.id(), &message).unwrap();

            message
        }

        pub fn owned_coins(&self, owner: &Address) -> Vec<Coin> {
            use crate::query::CoinQueryData;
            let db = self.service_database();
            db.owned_coins_ids(owner, None, IterDirection::Forward)
                .map(|res| res.map(|id| db.coin(id).unwrap()))
                .try_collect()
                .unwrap()
        }

        pub fn owned_messages(&self, owner: &Address) -> Vec<Message> {
            use crate::query::MessageQueryData;
            let db = self.service_database();
            db.owned_message_ids(owner, None, IterDirection::Forward)
                .map(|res| res.map(|id| db.message(&id).unwrap()))
                .try_collect()
                .unwrap()
        }
    }
}
