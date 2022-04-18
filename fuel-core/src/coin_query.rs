use crate::database::{Database, KvStoreError};
use crate::model::coin::{Coin, CoinStatus};
use crate::state::{self};
use fuel_storage::Storage;
use fuel_tx::{Address, AssetId, UtxoId};
use itertools::Itertools;
use rand::prelude::*;
use std::cmp::Reverse;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoinQueryError {
    #[error("store error occured")]
    KvStoreError(KvStoreError),
    #[error("state error occured")]
    StateError(state::Error),
    #[error("enough coins could not be found")]
    NotEnoughCoins,
    #[error("not enough inputs")]
    NotEnoughInputs,
}

impl From<KvStoreError> for CoinQueryError {
    fn from(e: KvStoreError) -> Self {
        CoinQueryError::KvStoreError(e)
    }
}

impl From<state::Error> for CoinQueryError {
    fn from(e: state::Error) -> Self {
        CoinQueryError::StateError(e)
    }
}

pub type SpendQuery = [SpendQueryElement];
pub type SpendQueryElement = (Address, AssetId, u64);

pub fn largest_first(
    db: &Database,
    spend_query: &SpendQuery,
    max_inputs: u8,
    excluded_ids: Option<&Vec<UtxoId>>,
) -> Result<Vec<(UtxoId, Coin)>, CoinQueryError> {
    // Merge elements with the same (owner, asset_id)
    let spend_query: Vec<SpendQueryElement> = spend_query
        .to_vec()
        .iter()
        .group_by(|(owner, asset_id, _)| (owner, asset_id))
        .into_iter()
        .map(|((owner, asset_id), group)| {
            (
                *owner,
                *asset_id,
                group.map(|(_, _, amount)| amount).sum::<u64>(),
            )
        })
        .collect();

    let mut coins: Vec<(UtxoId, Coin)> = vec![];

    for (owner, asset_id, amount) in spend_query {
        let coins_of_asset_id: Vec<(UtxoId, Coin)> = {
            let mut coin_ids: Vec<UtxoId> = db
                .owned_coins_by_asset_id(owner, asset_id, None, None)
                .try_collect()?;

            // Filter excluded coins
            coin_ids.retain(|&id| {
                excluded_ids
                    .map(|excluded_ids| !excluded_ids.contains(&id))
                    .unwrap_or(true)
            });

            let mut coins: Vec<(UtxoId, Coin)> = coin_ids
                .into_iter()
                .map(|id| {
                    Storage::<UtxoId, Coin>::get(db, &id)
                        .transpose()
                        .ok_or(KvStoreError::NotFound)?
                        .map(|coin| (id, coin.into_owned()))
                })
                .filter_ok(|(_, coin)| coin.status == CoinStatus::Unspent)
                .try_collect()?;

            coins.sort_by_key(|coin| Reverse(coin.1.amount));

            coins
        };

        let mut collected_amount = 0u64;

        for (id, coin) in coins_of_asset_id {
            // Break if we don't need any more coins
            if collected_amount >= amount {
                break;
            }

            // Error if we can't fit more coins
            if coins.len() >= max_inputs as usize {
                return Err(CoinQueryError::NotEnoughInputs);
            }

            // Add to list
            collected_amount += coin.amount;
            coins.push((id, coin));
        }

        if collected_amount < amount {
            return Err(CoinQueryError::NotEnoughCoins);
        }
    }

    Ok(coins)
}

// An implementation of the method described on: https://iohk.io/en/blog/posts/2018/07/03/self-organisation-in-coin-selection/
pub fn random_improve(
    db: &Database,
    spend_query: &SpendQuery,
    max_inputs: u8,
    excluded_ids: Option<&Vec<UtxoId>>,
) -> Result<Vec<(UtxoId, Coin)>, CoinQueryError> {
    // Merge elements with the same (owner, asset_id)
    let spend_query: Vec<SpendQueryElement> = spend_query
        .to_vec()
        .iter()
        .group_by(|(owner, asset_id, _)| (owner, asset_id))
        .into_iter()
        .map(|((owner, asset_id), group)| {
            (
                *owner,
                *asset_id,
                group.map(|(_, _, amount)| amount).sum::<u64>(),
            )
        })
        .collect();

    let mut coins: Vec<(UtxoId, Coin)> = vec![];

    let mut coins_by_asset_id: Vec<Vec<(UtxoId, Coin)>> = spend_query
        .iter()
        .map(|(owner, asset_id, _)| -> Result<_, CoinQueryError> {
            let mut coin_ids: Vec<UtxoId> = db
                .owned_coins_by_asset_id(*owner, *asset_id, None, None)
                .try_collect()?;

            // Filter excluded coins
            coin_ids.retain(|&id| {
                excluded_ids
                    .map(|excluded_ids| !excluded_ids.contains(&id))
                    .unwrap_or(true)
            });

            let coins: Vec<(UtxoId, Coin)> = coin_ids
                .into_iter()
                .map(|id| {
                    Storage::<UtxoId, Coin>::get(db, &id)
                        .transpose()
                        .ok_or(KvStoreError::NotFound)?
                        .map(|coin| (id, coin.into_owned()))
                })
                .filter_ok(|(_, coin)| coin.status == CoinStatus::Unspent)
                .try_collect()?;

            Ok(coins)
        })
        .try_collect()?;
    let mut collected_amounts: Vec<u64> = spend_query.iter().map(|_| 0).collect();

    // Collect enough coins to satisfy the spend query
    for (index, (_owner, _asset_id, amount)) in spend_query.iter().enumerate() {
        let coins_of_asset_id = &mut coins_by_asset_id[index];
        let collected_amount = &mut collected_amounts[index];

        loop {
            // Break if we don't need any more coins
            if *collected_amount >= *amount {
                break;
            }

            // Fallback to largest_first if we can't fit more coins
            if coins.len() >= max_inputs as usize {
                return largest_first(db, &spend_query, max_inputs, excluded_ids);
            }

            // Error if we don't have more coins
            if coins_of_asset_id.is_empty() {
                return Err(CoinQueryError::NotEnoughCoins);
            }

            // Remove random ID from the list
            let i = (0..coins_of_asset_id.len())
                .choose(&mut thread_rng())
                .unwrap();
            let coin = coins_of_asset_id.swap_remove(i);

            // Add to list
            *collected_amount += coin.1.amount;
            coins.push(coin);
        }
    }

    // Stop if we can't fit more coins
    if coins.len() >= max_inputs as usize {
        return Ok(coins);
    }

    // Collect extra coins to leave useful change
    for (index, (_owner, _asset_id, amount)) in spend_query.iter().enumerate() {
        let coins_of_asset_id = &mut coins_by_asset_id[index];
        let collected_amount = &mut collected_amounts[index];

        // Set parameters according to spec
        let target_amount = amount;
        let upper_limit = amount * 2;

        loop {
            // Break if we can't fit more coins
            if coins.len() >= max_inputs as usize {
                break;
            }

            // Break if we don't have more coins
            if coins_of_asset_id.is_empty() {
                break;
            }

            // Remove random ID from the list
            let i = (0..coins_of_asset_id.len())
                .choose(&mut thread_rng())
                .unwrap();
            let coin = coins_of_asset_id.swap_remove(i);

            // Break if found coin exceeds the upper limit
            if coin.1.amount > upper_limit {
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
            let change_amount = *collected_amount - amount;
            let distance = abs_diff(*target_amount, change_amount);
            let next_distance = abs_diff(*target_amount, change_amount + coin.1.amount);
            if next_distance >= distance {
                break;
            }

            // Add to list
            *collected_amount += coin.1.amount;
            coins.push(coin);
        }
    }

    Ok(coins)
}

#[cfg(test)]
mod tests {
    use assert_matches::assert_matches;
    use fuel_asm::Word;
    use fuel_tx::Address;

    use crate::test_utils::*;

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
        let query = |spend_query: &[SpendQueryElement],
                     max_inputs: u8|
         -> Result<Vec<(AssetId, u64)>, CoinQueryError> {
            let coins = largest_first(db.as_ref(), spend_query, max_inputs, None);

            // Transform result for convenience
            coins.map(|coins| {
                coins
                    .into_iter()
                    .map(|coin| (coin.1.asset_id, coin.1.amount))
                    .collect()
            })
        };

        // Query some amounts, including higher than the owner's balance
        for amount in 0..20 {
            let coins = query(&[(owner, asset_ids[0], amount)], u8::MAX);

            // Transform result for convenience
            let coins = coins.map(|coins| {
                coins
                    .into_iter()
                    .map(|(asset_id, amount)| {
                        // Check the asset ID before we drop it
                        assert_eq!(asset_id, asset_ids[0]);

                        amount
                    })
                    .collect::<Vec<u64>>()
            });

            match amount {
                // This should return nothing
                0 => assert_matches!(coins, Ok(coins) if coins.is_empty()),
                // This range should return the largest coin
                1..=5 => assert_matches!(coins, Ok(coins) if coins == vec![5]),
                // This range should return the largest two coins
                6..=9 => assert_matches!(coins, Ok(coins) if coins == vec![5, 4]),
                // This range should return the largest three coins
                10..=12 => assert_matches!(coins, Ok(coins) if coins == vec![5, 4, 3]),
                // This range should return the largest four coins
                13..=14 => assert_matches!(coins, Ok(coins) if coins == vec![5, 4, 3, 2]),
                // This range should return all coins
                15 => assert_matches!(coins, Ok(coins) if coins == vec![5, 4, 3, 2, 1]),
                // Asking for more than the owner's balance should error
                _ => assert_matches!(coins, Err(CoinQueryError::NotEnoughCoins)),
            };
        }

        // Query multiple asset IDs
        let coins = query(
            &[(owner, asset_ids[0], 3), (owner, asset_ids[1], 6)],
            u8::MAX,
        );
        assert_matches!(coins, Ok(coins) if coins == vec![(asset_ids[0], 5), (asset_ids[1], 5), (asset_ids[1], 4)]);

        // Query with duplicate (owner, asset_id)
        let coins = query(
            &[(owner, asset_ids[0], 3), (owner, asset_ids[0], 3)],
            u8::MAX,
        );
        assert_matches!(coins, Ok(coins) if coins == vec![(asset_ids[0], 5),  (asset_ids[0], 4)]);

        // Query with too small max_inputs
        let coins = query(&[(owner, asset_ids[0], 6)], 1);
        assert_matches!(coins, Err(CoinQueryError::NotEnoughInputs));
    }

    #[test]
    fn random_improve_output() {
        // Setup
        let owner = Address::default();
        let asset_ids = [AssetId::new([1u8; 32]), AssetId::new([2u8; 32])];
        let mut db = TestDatabase::default();
        (0..5usize).for_each(|i| {
            db.make_coin(owner, (i + 1) as Word, asset_ids[0]);
            db.make_coin(owner, (i + 1) as Word, asset_ids[1]);
        });
        let query = |spend_query: &[SpendQueryElement],
                     max_inputs: u8|
         -> Result<Vec<(AssetId, u64)>, CoinQueryError> {
            let coins = random_improve(db.as_ref(), spend_query, max_inputs, None);

            // Transform result for convenience
            coins.map(|coins| {
                coins
                    .into_iter()
                    .map(|coin| (coin.1.asset_id, coin.1.amount))
                    .sorted_by_key(|(asset_id, amount)| {
                        (
                            asset_ids.iter().position(|c| c == asset_id).unwrap(),
                            Reverse(*amount),
                        )
                    })
                    .collect()
            })
        };

        // Query some amounts, including higher than the owner's balance
        for amount in 0..20 {
            let coins = query(&[(owner, asset_ids[0], amount)], u8::MAX);

            // Transform result for convenience
            let coins = coins.map(|coins| {
                coins
                    .into_iter()
                    .map(|(asset_id, amount)| {
                        // Check the asset ID before we drop it
                        assert_eq!(asset_id, asset_ids[0]);

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
                8..=15 => assert_matches!(coins, Ok(coins) if coins == vec![5, 4, 3, 2, 1]),
                // Asking for more than the owner's balance should error
                _ => assert_matches!(coins, Err(CoinQueryError::NotEnoughCoins)),
            };
        }

        // Query multiple asset IDs
        let coins = query(&[(owner, asset_ids[0], 3), (owner, asset_ids[1], 6)], 3);
        assert_matches!(coins, Ok(ref coins) if coins.len() == 3);
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

        // Query with duplicate (owner, asset_id)
        let coins = query(
            &[(owner, asset_ids[0], 3), (owner, asset_ids[0], 3)],
            u8::MAX,
        );
        assert_matches!(coins, Ok(coins) if coins
            .iter()
            .filter(|c| c.0 == asset_ids[0])
            .map(|c| c.1)
            .sum::<u64>()
            >= 6);

        // Query with too small max_inputs
        let coins = query(&[(owner, asset_ids[0], 6)], 1);
        assert_matches!(coins, Err(CoinQueryError::NotEnoughInputs));
    }

    #[test]
    fn exclusion() {
        // Setup
        let owner = Address::default();
        let asset_ids = [AssetId::new([1u8; 32]), AssetId::new([2u8; 32])];
        let mut db = TestDatabase::default();
        (0..5usize).for_each(|i| {
            db.make_coin(owner, (i + 1) as Word, asset_ids[0]);
            db.make_coin(owner, (i + 1) as Word, asset_ids[1]);
        });
        let query = |spend_query: &[SpendQueryElement],
                     max_inputs: u8,
                     excluded_ids: Option<&Vec<UtxoId>>|
         -> Result<Vec<(AssetId, u64)>, CoinQueryError> {
            let coins = random_improve(db.as_ref(), spend_query, max_inputs, excluded_ids);

            // Transform result for convenience
            coins.map(|coins| {
                coins
                    .into_iter()
                    .map(|coin| (coin.1.asset_id, coin.1.amount))
                    .sorted_by_key(|(asset_id, amount)| {
                        (
                            asset_ids.iter().position(|c| c == asset_id).unwrap(),
                            Reverse(*amount),
                        )
                    })
                    .collect()
            })
        };

        // Exclude largest coin IDs
        let excluded_ids = db
            .owned_coins(owner)
            .into_iter()
            .filter(|(_, coin)| coin.amount == 5)
            .map(|(utxo_id, _)| utxo_id)
            .collect();

        // Query some amounts, including higher than the owner's balance
        for amount in 0..20 {
            let coins = query(
                &[(owner, asset_ids[0], amount)],
                u8::MAX,
                Some(&excluded_ids),
            );

            // Transform result for convenience
            let coins = coins.map(|coins| {
                coins
                    .into_iter()
                    .map(|(asset_id, amount)| {
                        // Check the asset ID before we drop it
                        assert_eq!(asset_id, asset_ids[0]);

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
                5..=10 => assert_matches!(coins, Ok(coins) if coins == vec![ 4, 3, 2, 1]),
                // Asking for more than the owner's balance should error
                _ => assert_matches!(coins, Err(CoinQueryError::NotEnoughCoins)),
            };
        }
    }
}
