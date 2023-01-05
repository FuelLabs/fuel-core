use crate::database::{
    resource::{
        AssetQuery,
        AssetSpendTarget,
        Exclude,
        Resource,
        ResourceId,
    },
    Database,
};
use core::mem::swap;
use fuel_core_storage::Error as StorageError;
use fuel_core_types::{
    entities::{
        coin::Coin,
        message::Message,
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
pub enum ResourceQueryError {
    #[error("store error occurred")]
    StorageError(StorageError),
    #[error("not enough resources to fit the target")]
    InsufficientResources {
        asset_id: AssetId,
        collected_amount: Word,
    },
    #[error("max number of resources is reached while trying to fit the target")]
    MaxResourcesReached,
    #[error("the query contains duplicate assets")]
    DuplicateAssets(AssetId),
}

#[cfg(test)]
impl PartialEq for ResourceQueryError {
    fn eq(&self, other: &Self) -> bool {
        format!("{:?}", self) == format!("{:?}", other)
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
        exclude_vec: Option<Vec<ResourceId>>,
    ) -> Result<Self, ResourceQueryError> {
        let mut duplicate_checker = HashSet::new();

        for query in query_per_asset {
            if duplicate_checker.contains(&query.id) {
                return Err(ResourceQueryError::DuplicateAssets(query.id))
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
pub fn largest_first(
    query: &AssetQuery,
) -> Result<Vec<Resource<Coin, Message>>, ResourceQueryError> {
    let mut inputs: Vec<_> = query.unspent_resources().try_collect()?;
    inputs.sort_by_key(|resource| Reverse(*resource.amount()));

    let mut collected_amount = 0u64;
    let mut resources = vec![];

    for resource in inputs {
        // Break if we don't need any more coins
        if collected_amount >= query.asset.target {
            break
        }

        // Error if we can't fit more coins
        if resources.len() >= query.asset.max {
            return Err(ResourceQueryError::MaxResourcesReached)
        }

        // Add to list
        collected_amount = collected_amount.saturating_add(*resource.amount());
        resources.push(resource.into_owned());
    }

    if collected_amount < query.asset.target {
        return Err(ResourceQueryError::InsufficientResources {
            asset_id: query.asset.id,
            collected_amount,
        })
    }

    Ok(resources)
}

// An implementation of the method described on: https://iohk.io/en/blog/posts/2018/07/03/self-organisation-in-coin-selection/
pub fn random_improve(
    db: &Database,
    spend_query: &SpendQuery,
) -> Result<Vec<Vec<Resource<Coin, Message>>>, ResourceQueryError> {
    let mut resources_per_asset = vec![];

    for query in spend_query.asset_queries(db) {
        let mut inputs: Vec<_> = query.unspent_resources().try_collect()?;
        inputs.shuffle(&mut thread_rng());
        inputs.truncate(query.asset.max);

        let mut collected_amount = 0;
        let mut resources = vec![];

        // Set parameters according to spec
        let target = query.asset.target;
        let upper_target = query.asset.target.saturating_mul(2);

        for resource in inputs {
            // Try to improve the result by adding dust to the result.
            if collected_amount >= target {
                // Break if found resource exceeds max `u64` or the upper limit
                if collected_amount == u64::MAX || resource.amount() > &upper_target {
                    break
                }

                // Break if adding doesn't improve the distance
                let change_amount = collected_amount - target;
                let distance = target.abs_diff(change_amount);
                let next_distance = target.abs_diff(change_amount + resource.amount());
                if next_distance >= distance {
                    break
                }
            }

            // Add to list
            collected_amount = collected_amount.saturating_add(*resource.amount());
            resources.push(resource.into_owned());
        }

        // Fallback to largest_first if we can't fit more coins
        if collected_amount < query.asset.target {
            swap(&mut resources, &mut largest_first(&query)?);
        }

        resources_per_asset.push(resources);
    }

    Ok(resources_per_asset)
}

impl From<StorageError> for ResourceQueryError {
    fn from(e: StorageError) -> Self {
        ResourceQueryError::StorageError(e)
    }
}

#[cfg(test)]
mod tests {
    use crate::database::Database;
    use assert_matches::assert_matches;
    use fuel_core_storage::{
        tables::{
            Coins,
            Messages,
        },
        StorageAsMut,
        StorageAsRef,
    };
    use fuel_core_types::{
        blockchain::primitives::DaBlockHeight,
        entities::coin::CoinStatus,
        fuel_asm::Word,
        fuel_tx::*,
    };
    use itertools::Itertools;

    use super::*;

    fn setup_coins() -> (Address, [AssetId; 2], TestDatabase) {
        let owner = Address::default();
        let asset_ids = [AssetId::new([1u8; 32]), AssetId::new([2u8; 32])];
        let mut db = TestDatabase::default();
        (0..5usize).for_each(|i| {
            db.make_coin(owner, (i + 1) as Word, asset_ids[0]);
            db.make_coin(owner, (i + 1) as Word, asset_ids[1]);
        });

        (owner, asset_ids, db)
    }

    fn setup_messages() -> (Address, AssetId, TestDatabase) {
        let owner = Address::default();
        let asset_id = AssetId::BASE;
        let mut db = TestDatabase::default();
        (0..5usize).for_each(|i| {
            db.make_message(owner, (i + 1) as Word);
        });

        (owner, asset_id, db)
    }

    fn setup_coins_and_messages() -> (Address, [AssetId; 2], TestDatabase) {
        let owner = Address::default();
        let asset_ids = [AssetId::BASE, AssetId::new([1u8; 32])];
        let mut db = TestDatabase::default();
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
            db: &Database,
        ) -> Result<Vec<Vec<(AssetId, Word)>>, ResourceQueryError> {
            let result: Vec<_> = spend_query
                .iter()
                .map(|asset| {
                    largest_first(&AssetQuery::new(owner, asset, None, db)).map(
                        |resources| {
                            resources
                                .iter()
                                .map(|resource| {
                                    (*resource.asset_id(), *resource.amount())
                                })
                                .collect()
                        },
                    )
                })
                .try_collect()?;
            Ok(result)
        }

        fn single_asset_assert(owner: Address, asset_ids: &[AssetId], db: TestDatabase) {
            let asset_id = asset_ids[0];

            // Query some targets, including higher than the owner's balance
            for target in 0..20 {
                let resources = query(
                    &[AssetSpendTarget::new(asset_id, target, u64::MAX)],
                    &owner,
                    db.as_ref(),
                );

                // Transform result for convenience
                let resources = resources.map(|resources| {
                    resources[0]
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
                        assert_matches!(resources, Ok(resources) if resources.is_empty())
                    }
                    // This range should return the largest resources
                    1..=5 => {
                        assert_matches!(resources, Ok(resources) if resources == vec![5])
                    }
                    // This range should return the largest two resources
                    6..=9 => {
                        assert_matches!(resources, Ok(resources) if resources == vec![5, 4])
                    }
                    // This range should return the largest three resources
                    10..=12 => {
                        assert_matches!(resources, Ok(resources) if resources == vec![5, 4, 3])
                    }
                    // This range should return the largest four resources
                    13..=14 => {
                        assert_matches!(resources, Ok(resources) if resources == vec![5, 4, 3, 2])
                    }
                    // This range should return all resources
                    15 => {
                        assert_matches!(resources, Ok(resources) if resources == vec![5, 4, 3, 2, 1])
                    }
                    // Asking for more than the owner's balance should error
                    _ => {
                        assert_matches!(
                            resources,
                            Err(ResourceQueryError::InsufficientResources {
                                asset_id: _,
                                collected_amount: 15,
                            })
                        )
                    }
                };
            }

            // Query with too small max_inputs
            let resources = query(
                &[AssetSpendTarget::new(asset_id, 6, 1)],
                &owner,
                db.as_ref(),
            );
            assert_matches!(resources, Err(ResourceQueryError::MaxResourcesReached));
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
            let resources = query(
                &[
                    AssetSpendTarget::new(asset_ids[0], 3, u64::MAX),
                    AssetSpendTarget::new(asset_ids[1], 6, u64::MAX),
                ],
                &owner,
                db.as_ref(),
            );
            assert_matches!(resources, Ok(resources)
            if resources == vec![
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
            db: &Database,
        ) -> Result<Vec<(AssetId, u64)>, ResourceQueryError> {
            let coins =
                random_improve(db, &SpendQuery::new(owner, &query_per_asset, None)?);

            // Transform result for convenience
            coins.map(|coins| {
                coins
                    .into_iter()
                    .flat_map(|coin| {
                        coin.into_iter()
                            .map(|resource| (*resource.asset_id(), *resource.amount()))
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
                    db.as_ref(),
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
                            Err(ResourceQueryError::InsufficientResources {
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
                db.as_ref(),
            );
            assert_matches!(coins, Err(ResourceQueryError::MaxResourcesReached));
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
                db.as_ref(),
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

        fn exclusion_assert(
            owner: Address,
            asset_ids: &[AssetId],
            db: TestDatabase,
            excluded_ids: Vec<ResourceId>,
        ) {
            let asset_id = asset_ids[0];

            let query = |query_per_asset: Vec<AssetSpendTarget>,
                         excluded_ids: Vec<ResourceId>|
             -> Result<Vec<(AssetId, u64)>, ResourceQueryError> {
                let coins = random_improve(
                    db.as_ref(),
                    &SpendQuery::new(owner, &query_per_asset, Some(excluded_ids))?,
                );

                // Transform result for convenience
                coins.map(|coins| {
                    coins
                        .into_iter()
                        .flat_map(|coin| {
                            coin.into_iter()
                                .map(|resource| {
                                    (*resource.asset_id(), *resource.amount())
                                })
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
                            Err(ResourceQueryError::InsufficientResources {
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
                .filter(|(_, coin)| coin.amount == 5)
                .map(|(utxo_id, _)| ResourceId::Utxo(utxo_id))
                .collect_vec();

            exclusion_assert(owner, &asset_ids, db, excluded_ids);

            // Setup messages
            let (owner, asset_ids, db) = setup_messages();

            // Exclude largest messages IDs
            let excluded_ids = db
                .owned_messages(&owner)
                .into_iter()
                .filter(|message| message.amount == 5)
                .map(|message| ResourceId::Message(message.id()))
                .collect_vec();

            exclusion_assert(owner, &[asset_ids], db, excluded_ids);

            // Setup coins and messages
            let (owner, asset_ids, db) = setup_coins_and_messages();

            // Exclude largest messages IDs, because coins only 1 and 2
            let excluded_ids = db
                .owned_messages(&owner)
                .into_iter()
                .filter(|message| message.amount == 5)
                .map(|message| ResourceId::Message(message.id()))
                .collect_vec();

            exclusion_assert(owner, &asset_ids, db, excluded_ids);
        }
    }

    #[derive(Clone, Debug)]
    struct TestCase {
        db_amount: Vec<Word>,
        target_amount: u64,
        max_resources: usize,
    }

    #[test_case::test_case(
        TestCase {
            db_amount: vec![0],
            target_amount: u64::MAX,
            max_resources: usize::MAX,
        }
        => Err(ResourceQueryError::InsufficientResources {
            asset_id: AssetId::BASE, collected_amount: 0
        })
        ; "Insufficient resources in the DB(0) to reach target(u64::MAX)"
    )]
    #[test_case::test_case(
        TestCase {
            db_amount: vec![u64::MAX, u64::MAX],
            target_amount: u64::MAX,
            max_resources: usize::MAX,
        }
        => Ok(1)
        ; "Enough resources in the DB to reach target(u64::MAX) by 1 resource"
    )]
    #[test_case::test_case(
        TestCase {
            db_amount: vec![2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, u64::MAX - 1],
            target_amount: u64::MAX,
            max_resources: 2,
        }
        => Ok(2)
        ; "Enough resources in the DB to reach target(u64::MAX) by 2 resource"
    )]
    #[test_case::test_case(
        TestCase {
            db_amount: vec![u64::MAX, u64::MAX],
            target_amount: u64::MAX,
            max_resources: 0,
        }
        => Err(ResourceQueryError::MaxResourcesReached)
        ; "Enough resources in the DB to reach target(u64::MAX) but limit is zero"
    )]
    fn corner_cases(case: TestCase) -> Result<usize, ResourceQueryError> {
        pub enum ResourceType {
            Coin,
            Message,
        }

        fn test_case_run(
            case: TestCase,
            resource_type: ResourceType,
        ) -> Result<usize, ResourceQueryError> {
            let TestCase {
                db_amount,
                target_amount,
                max_resources,
            } = case;
            let owner = Address::default();
            let asset_ids = vec![AssetId::BASE];
            let mut db = TestDatabase::default();
            for amount in db_amount {
                match resource_type {
                    ResourceType::Coin => {
                        let _ = db.make_coin(owner, amount, asset_ids[0]);
                    }
                    ResourceType::Message => {
                        let _ = db.make_message(owner, amount);
                    }
                };
            }

            let coins = random_improve(
                db.as_ref(),
                &SpendQuery::new(
                    owner,
                    &[AssetSpendTarget {
                        id: asset_ids[0],
                        target: target_amount,
                        max: max_resources,
                    }],
                    None,
                )?,
            )?;

            assert_eq!(coins.len(), 1);
            Ok(coins[0].len())
        }

        let coin_result = test_case_run(case.clone(), ResourceType::Coin);
        let message_result = test_case_run(case, ResourceType::Message);
        assert_eq!(coin_result, message_result);

        coin_result
    }

    #[derive(Default)]
    pub struct TestDatabase {
        database: Database,
        last_coin_index: u64,
        last_message_index: u64,
    }

    impl TestDatabase {
        pub fn make_coin(
            &mut self,
            owner: Address,
            amount: Word,
            asset_id: AssetId,
        ) -> (UtxoId, Coin) {
            let index = self.last_coin_index;
            self.last_coin_index += 1;

            let id = UtxoId::new(Bytes32::from([0u8; 32]), index.try_into().unwrap());
            let coin = Coin {
                owner,
                amount,
                asset_id,
                maturity: Default::default(),
                status: CoinStatus::Unspent,
                block_created: Default::default(),
            };

            let db = &mut self.database;
            db.storage::<Coins>().insert(&id, &coin).unwrap();

            (id, coin)
        }

        pub fn make_message(
            &mut self,
            owner: Address,
            amount: Word,
        ) -> (MessageId, Message) {
            let nonce = self.last_message_index;
            self.last_message_index += 1;

            let message = Message {
                sender: Default::default(),
                recipient: owner,
                nonce,
                amount,
                data: vec![],
                da_height: DaBlockHeight::from(1u64),
                fuel_block_spend: None,
            };

            let db = &mut self.database;
            db.storage::<Messages>()
                .insert(&message.id(), &message)
                .unwrap();

            (message.id(), message)
        }

        pub fn owned_coins(&self, owner: &Address) -> Vec<(UtxoId, Coin)> {
            self.database
                .owned_coins_ids(owner, None, None)
                .map(|res| {
                    res.map(|id| {
                        let coin =
                            self.database.storage::<Coins>().get(&id).unwrap().unwrap();
                        (id, coin.into_owned())
                    })
                })
                .try_collect()
                .unwrap()
        }

        pub fn owned_messages(&self, owner: &Address) -> Vec<Message> {
            self.database
                .owned_message_ids(owner, None, None)
                .map(|res| {
                    res.map(|id| {
                        let message = self
                            .database
                            .storage::<Messages>()
                            .get(&id)
                            .unwrap()
                            .unwrap();
                        message.into_owned()
                    })
                })
                .try_collect()
                .unwrap()
        }
    }

    impl AsRef<Database> for TestDatabase {
        fn as_ref(&self) -> &Database {
            self.database.as_ref()
        }
    }
}
