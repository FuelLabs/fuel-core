use crate::database::utils::{Asset, AssetQuery, Banknote, BanknoteId, Excluder};
use crate::database::{Database, KvStoreError};
use crate::model::Coin;
use crate::state::Error as StateError;
use core::mem::swap;
use fuel_core_interfaces::common::fuel_tx::Address;
use fuel_core_interfaces::model::Message;
use itertools::Itertools;
use rand::prelude::*;
use std::cmp::Reverse;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoinQueryError {
    #[error("store error occurred")]
    KvStoreError(KvStoreError),
    #[error("state error occurred")]
    StateError(StateError),
    #[error("enough coins could not be found")]
    NotEnoughCoins,
    #[error("not enough inputs")]
    NotEnoughInputs,
}

/// The prepared spend queries.
pub struct SpendQuery {
    owner: Address,
    /// Each `(asset_id, _)` is unique in the vector after construction in the `new`.
    assets: Vec<Asset>,
    excluder: Excluder,
    max_inputs: usize,
}

impl SpendQuery {
    // TODO: Check that number of `queries` is not too high(to prevent attacks).
    pub fn new(
        owner: Address,
        assets: &[Asset],
        exclude_vec: Option<Vec<BanknoteId>>,
        max_inputs: Option<u64>,
    ) -> Self {
        let assets = assets
            .iter()
            .group_by(|asset| asset.id)
            .into_iter()
            .map(|(asset_id, group)| {
                Asset::new(asset_id, group.map(|asset| asset.target).sum::<u64>())
            })
            .collect();

        let excluder = if let Some(exclude_vec) = exclude_vec {
            Excluder::new(exclude_vec)
        } else {
            Default::default()
        };

        Self {
            owner,
            assets,
            excluder,
            max_inputs: max_inputs.unwrap_or(u64::MAX) as usize,
        }
    }

    /// Return [`Asset`]s grouped by `asset_id`.
    pub fn assets(&self) -> &Vec<Asset> {
        &self.assets
    }

    /// Return [`Asset`]s grouped by `asset_id`.
    pub fn asset_queries<'a>(&'a self, db: &'a Database) -> Vec<AssetQuery<'a>> {
        self.assets
            .iter()
            .map(|asset| AssetQuery::new(&self.owner, asset, Some(&self.excluder), db))
            .collect()
    }

    /// Returns excluder that contains information about excluded ids.
    pub fn excluder(&self) -> &Excluder {
        &self.excluder
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
    max_input: usize,
) -> Result<Vec<Banknote<Coin, Message>>, CoinQueryError> {
    let mut inputs: Vec<_> = query.unspent_inputs().try_collect()?;
    inputs.sort_by_key(|banknote| Reverse(*banknote.amount()));

    let mut collected_amount = 0u64;
    let mut banknotes = vec![];

    for coin in inputs {
        // Break if we don't need any more coins
        if collected_amount >= query.asset.target {
            break;
        }

        // Error if we can't fit more coins
        if banknotes.len() >= max_input {
            return Err(CoinQueryError::NotEnoughInputs);
        }

        // Add to list
        collected_amount += coin.amount();
        banknotes.push(coin.into_owned());
    }

    if collected_amount < query.asset.target {
        // TODO: Return the asset id and maybe collected amount
        return Err(CoinQueryError::NotEnoughCoins);
    }

    Ok(banknotes)
}

// An implementation of the method described on: https://iohk.io/en/blog/posts/2018/07/03/self-organisation-in-coin-selection/
pub fn random_improve(
    db: &Database,
    spend_query: &SpendQuery,
) -> Result<Vec<Vec<Banknote<Coin, Message>>>, CoinQueryError> {
    let mut banknotes_per_asset = vec![];

    for query in spend_query.asset_queries(db) {
        let mut inputs: Vec<_> = query.unspent_inputs().try_collect()?;
        inputs.shuffle(&mut thread_rng());
        inputs.truncate(spend_query.max_inputs as usize);

        let mut collected_amount = 0;
        let mut banknotes = vec![];

        // Set parameters according to spec
        let target = query.asset.target;
        let upper_target = query.asset.target * 2;

        for banknote in inputs {
            // Try to improve the result by adding dust to the result.
            if collected_amount >= target {
                // Break if found banknote exceeds the upper limit
                if banknote.amount() > &upper_target {
                    break;
                }

                // Break if adding doesn't improve the distance
                // TODO: Use stable abs_diff when available https://github.com/rust-lang/rust/issues/89492
                let abs_diff = |a: u64, b: u64| -> u64 {
                    if a > b {
                        a - b
                    } else {
                        b - a
                    }
                };
                let change_amount = collected_amount - target;
                let distance = abs_diff(target, change_amount);
                let next_distance = abs_diff(target, change_amount + banknote.amount());
                if next_distance >= distance {
                    break;
                }
            }

            // Add to list
            collected_amount += banknote.amount();
            banknotes.push(banknote.into_owned());
        }

        // Fallback to largest_first if we can't fit more coins
        if collected_amount < query.asset.target {
            swap(
                &mut banknotes,
                &mut largest_first(&query, spend_query.max_inputs)?,
            );
        }

        banknotes_per_asset.push(banknotes);
    }

    Ok(banknotes_per_asset)
}

#[cfg(test)]
mod tests {
    use crate::test_utils::*;
    use assert_matches::assert_matches;
    use fuel_core_interfaces::common::fuel_types::AssetId;
    use fuel_core_interfaces::common::{fuel_asm::Word, fuel_tx::Address};

    use super::*;

    #[test]
    fn largest_first_output() {
        // Setup
        let owner = Address::default();
        let asset_ids = [AssetId::new([1u8; 32]), AssetId::new([2u8; 32])];
        let mut db = TestDatabase::default();
        (0..5usize).for_each(|i| {
            db.make_coin(owner, (i + 1) as Word, asset_ids[0]);
            db.make_coin(owner, (i + 1) as Word, asset_ids[1]);
        });
        let query = |spend_query: &[Asset],
                     max_input: usize|
         -> Result<Vec<Vec<(AssetId, Word)>>, CoinQueryError> {
            let result: Vec<_> = spend_query
                .iter()
                .map(|asset| {
                    largest_first(
                        &AssetQuery::new(&owner, &asset, None, db.as_ref()),
                        max_input,
                    )
                    .map(|banknotes| {
                        banknotes
                            .iter()
                            .map(|banknote| (*banknote.asset_id(), *banknote.amount()))
                            .collect()
                    })
                })
                .try_collect()?;
            Ok(result)
        };

        // Query some targets, including higher than the owner's balance
        for target in 0..20 {
            let banknotes = query(&[Asset::new(asset_ids[0], target)], usize::MAX);

            // Transform result for convenience
            let banknotes = banknotes.map(|banknotes| {
                banknotes[0]
                    .iter()
                    .map(|(asset_id, amount)| {
                        // Check the asset ID before we drop it
                        assert_eq!(asset_id, &asset_ids[0]);

                        *amount
                    })
                    .collect::<Vec<u64>>()
            });

            match target {
                // This should return nothing
                0 => assert_matches!(banknotes, Ok(banknotes) if banknotes.is_empty()),
                // This range should return the largest banknotes
                1..=5 => assert_matches!(banknotes, Ok(banknotes) if banknotes == vec![5]),
                // This range should return the largest two banknotes
                6..=9 => assert_matches!(banknotes, Ok(banknotes) if banknotes == vec![5, 4]),
                // This range should return the largest three banknotes
                10..=12 => assert_matches!(banknotes, Ok(banknotes) if banknotes == vec![5, 4, 3]),
                // This range should return the largest four banknotes
                13..=14 => {
                    assert_matches!(banknotes, Ok(banknotes) if banknotes == vec![5, 4, 3, 2])
                }
                // This range should return all banknotes
                15 => assert_matches!(banknotes, Ok(banknotes) if banknotes == vec![5, 4, 3, 2, 1]),
                // Asking for more than the owner's balance should error
                _ => assert_matches!(banknotes, Err(CoinQueryError::NotEnoughCoins)),
            };
        }

        // Query multiple asset IDs
        let banknotes = query(
            &[Asset::new(asset_ids[0], 3), Asset::new(asset_ids[1], 6)],
            usize::MAX,
        );
        assert_matches!(banknotes, Ok(banknotes)
        if banknotes == vec![
            vec![(asset_ids[0], 5)],
            vec![(asset_ids[1], 5), (asset_ids[1], 4)]
        ]);

        // Query with too small max_inputs
        let banknotes = query(&[Asset::new(asset_ids[0], 6)], 1);
        assert_matches!(banknotes, Err(CoinQueryError::NotEnoughInputs));
    }

    // #[test]
    // fn random_improve_output() {
    //     // Setup
    //     let owner = Address::default();
    //     let asset_ids = [AssetId::new([1u8; 32]), AssetId::new([2u8; 32])];
    //     let mut db = TestDatabase::default();
    //     (0..5usize).for_each(|i| {
    //         db.make_coin(owner, (i + 1) as Word, asset_ids[0]);
    //         db.make_coin(owner, (i + 1) as Word, asset_ids[1]);
    //     });
    //     let query = |spend_query, max_inputs: u64| -> Result<Vec<(AssetId, u64)>, CoinQueryError> {
    //         let coins = random_improve(db.as_ref(), spend_query, max_inputs, None);
    //
    //         // Transform result for convenience
    //         coins.map(|coins| {
    //             coins
    //                 .into_iter()
    //                 .map(|coin| (coin.1.asset_id, coin.1.amount))
    //                 .sorted_by_key(|(asset_id, amount)| {
    //                     (
    //                         asset_ids.iter().position(|c| c == asset_id).unwrap(),
    //                         Reverse(*amount),
    //                     )
    //                 })
    //                 .collect()
    //         })
    //     };
    //
    //     // Query some amounts, including higher than the owner's balance
    //     for amount in 0..20 {
    //         let coins = query(&[(owner, asset_ids[0], amount)], u8::MAX as u64);
    //
    //         // Transform result for convenience
    //         let coins = coins.map(|coins| {
    //             coins
    //                 .into_iter()
    //                 .map(|(asset_id, amount)| {
    //                     // Check the asset ID before we drop it
    //                     assert_eq!(asset_id, asset_ids[0]);
    //
    //                     amount
    //                 })
    //                 .collect::<Vec<u64>>()
    //         });
    //
    //         match amount {
    //             // This should return nothing
    //             0 => assert_matches!(coins, Ok(coins) if coins.is_empty()),
    //             // This range should...
    //             1..=7 => {
    //                 // ...satisfy the amount
    //                 assert_matches!(coins, Ok(coins) if coins.iter().sum::<u64>() >= amount)
    //                 // ...and add more for dust management
    //                 // TODO: Implement the test
    //             }
    //             // This range should return all coins
    //             8..=15 => assert_matches!(coins, Ok(coins) if coins == vec![5, 4, 3, 2, 1]),
    //             // Asking for more than the owner's balance should error
    //             _ => assert_matches!(coins, Err(CoinQueryError::NotEnoughCoins)),
    //         };
    //     }
    //
    //     // Query multiple asset IDs
    //     let coins = query(&[(owner, asset_ids[0], 3), (owner, asset_ids[1], 6)], 3);
    //     assert_matches!(coins, Ok(ref coins) if coins.len() == 3);
    //     let coins = coins.unwrap();
    //     assert!(
    //         coins
    //             .iter()
    //             .filter(|c| c.0 == asset_ids[0])
    //             .map(|c| c.1)
    //             .sum::<u64>()
    //             >= 3
    //     );
    //     assert!(
    //         coins
    //             .iter()
    //             .filter(|c| c.0 == asset_ids[1])
    //             .map(|c| c.1)
    //             .sum::<u64>()
    //             >= 6
    //     );
    //
    //     // Query with duplicate (owner, asset_id)
    //     let coins = query(
    //         &[(owner, asset_ids[0], 3), (owner, asset_ids[0], 3)],
    //         u8::MAX as u64,
    //     );
    //     assert_matches!(coins, Ok(coins) if coins
    //         .iter()
    //         .filter(|c| c.0 == asset_ids[0])
    //         .map(|c| c.1)
    //         .sum::<u64>()
    //         >= 6);
    //
    //     // Query with too small max_inputs
    //     let coins = query(&[(owner, asset_ids[0], 6)], 1);
    //     assert_matches!(coins, Err(CoinQueryError::NotEnoughInputs));
    // }
    //
    // #[test]
    // fn exclusion() {
    //     // Setup
    //     let owner = Address::default();
    //     let asset_ids = [AssetId::new([1u8; 32]), AssetId::new([2u8; 32])];
    //     let mut db = TestDatabase::default();
    //     (0..5usize).for_each(|i| {
    //         db.make_coin(owner, (i + 1) as Word, asset_ids[0]);
    //         db.make_coin(owner, (i + 1) as Word, asset_ids[1]);
    //     });
    //     let query = |spend_query,
    //                  max_inputs: u64,
    //                  excluded_ids: Option<&Vec<UtxoId>>|
    //      -> Result<Vec<(AssetId, u64)>, CoinQueryError> {
    //         let coins = random_improve(db.as_ref(), spend_query, max_inputs, excluded_ids);
    //
    //         // Transform result for convenience
    //         coins.map(|coins| {
    //             coins
    //                 .into_iter()
    //                 .map(|coin| (coin.1.asset_id, coin.1.amount))
    //                 .sorted_by_key(|(asset_id, amount)| {
    //                     (
    //                         asset_ids.iter().position(|c| c == asset_id).unwrap(),
    //                         Reverse(*amount),
    //                     )
    //                 })
    //                 .collect()
    //         })
    //     };
    //
    //     // Exclude largest coin IDs
    //     let excluded_ids = db
    //         .owned_coins(owner)
    //         .into_iter()
    //         .filter(|(_, coin)| coin.amount == 5)
    //         .map(|(utxo_id, _)| utxo_id)
    //         .collect();
    //
    //     // Query some amounts, including higher than the owner's balance
    //     for amount in 0..20 {
    //         let coins = query(
    //             &[(owner, asset_ids[0], amount)],
    //             u8::MAX as u64,
    //             Some(&excluded_ids),
    //         );
    //
    //         // Transform result for convenience
    //         let coins = coins.map(|coins| {
    //             coins
    //                 .into_iter()
    //                 .map(|(asset_id, amount)| {
    //                     // Check the asset ID before we drop it
    //                     assert_eq!(asset_id, asset_ids[0]);
    //
    //                     amount
    //                 })
    //                 .collect::<Vec<u64>>()
    //         });
    //
    //         match amount {
    //             // This should return nothing
    //             0 => assert_matches!(coins, Ok(coins) if coins.is_empty()),
    //             // This range should...
    //             1..=4 => {
    //                 // ...satisfy the amount
    //                 assert_matches!(coins, Ok(coins) if coins.iter().sum::<u64>() >= amount)
    //                 // ...and add more for dust management
    //                 // TODO: Implement the test
    //             }
    //             // This range should return all coins
    //             5..=10 => assert_matches!(coins, Ok(coins) if coins == vec![ 4, 3, 2, 1]),
    //             // Asking for more than the owner's balance should error
    //             _ => assert_matches!(coins, Err(CoinQueryError::NotEnoughCoins)),
    //         };
    //     }
    // }
}

impl From<KvStoreError> for CoinQueryError {
    fn from(e: KvStoreError) -> Self {
        CoinQueryError::KvStoreError(e)
    }
}

impl From<StateError> for CoinQueryError {
    fn from(e: StateError) -> Self {
        CoinQueryError::StateError(e)
    }
}
