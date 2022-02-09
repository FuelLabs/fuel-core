use crate::database::{Database, KvStoreError};
use crate::model::coin::{Coin, CoinStatus};
use crate::state::{self};
use fuel_storage::Storage;
use fuel_tx::{Address, Color, UtxoId};
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
pub type SpendQueryElement = (Address, Color, u64);

pub fn largest_first(
    db: &Database,
    spend_query: &SpendQuery,
    max_inputs: u8,
) -> Result<Vec<(UtxoId, Coin)>, CoinQueryError> {
    // Merge elements with the same (owner, color)
    let spend_query: Vec<SpendQueryElement> = spend_query
        .to_vec()
        .iter()
        .group_by(|(owner, color, _)| (owner, color))
        .into_iter()
        .map(|((owner, color), group)| {
            (
                *owner,
                *color,
                group.map(|(_, _, amount)| amount).sum::<u64>(),
            )
        })
        .collect();

    let mut coins: Vec<(UtxoId, Coin)> = vec![];

    for (owner, color, amount) in spend_query {
        let coins_of_color: Vec<(UtxoId, Coin)> = {
            let coin_ids: Vec<UtxoId> = db
                .owned_coins_by_color(owner, color, None, None)
                .try_collect()?;

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

        for (id, coin) in coins_of_color {
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
) -> Result<Vec<(UtxoId, Coin)>, CoinQueryError> {
    // Merge elements with the same (owner, color)
    let spend_query: Vec<SpendQueryElement> = spend_query
        .to_vec()
        .iter()
        .group_by(|(owner, color, _)| (owner, color))
        .into_iter()
        .map(|((owner, color), group)| {
            (
                *owner,
                *color,
                group.map(|(_, _, amount)| amount).sum::<u64>(),
            )
        })
        .collect();

    let mut coins: Vec<(UtxoId, Coin)> = vec![];

    let mut coins_by_color: Vec<Vec<(UtxoId, Coin)>> = spend_query
        .iter()
        .map(|(owner, color, _)| -> Result<_, CoinQueryError> {
            let coin_ids: Vec<UtxoId> = db
                .owned_coins_by_color(*owner, *color, None, None)
                .try_collect()?;

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
    for (index, (_owner, _color, amount)) in spend_query.iter().enumerate() {
        let coins_of_color = &mut coins_by_color[index];
        let collected_amount = &mut collected_amounts[index];

        loop {
            // Break if we don't need any more coins
            if *collected_amount >= *amount {
                break;
            }

            // Fallback to largest_first if we can't fit more coins
            if coins.len() >= max_inputs as usize {
                return largest_first(db, &spend_query, max_inputs);
            }

            // Error if we don't have more coins
            if coins_of_color.is_empty() {
                return Err(CoinQueryError::NotEnoughCoins);
            }

            // Remove random ID from the list
            let i = (0..coins_of_color.len()).choose(&mut thread_rng()).unwrap();
            let coin = coins_of_color.swap_remove(i);

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
    for (index, (_owner, _color, amount)) in spend_query.iter().enumerate() {
        let coins_of_color = &mut coins_by_color[index];
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
            if coins_of_color.is_empty() {
                break;
            }

            // Remove random ID from the list
            let i = (0..coins_of_color.len()).choose(&mut thread_rng()).unwrap();
            let coin = coins_of_color.swap_remove(i);

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
    use fuel_tx::{Address, Bytes32};

    use crate::model::coin::CoinStatus;

    use super::*;

    fn gen_test_db(owner: Address, colors: &[Color]) -> Database {
        let mut db = Database::default();

        let coins: Vec<(UtxoId, Coin)> = colors
            .iter()
            .flat_map(|color| {
                (0..5usize).map(move |i| Coin {
                    owner,
                    amount: (i + 1) as Word,
                    color: *color,
                    maturity: Default::default(),
                    status: CoinStatus::Unspent,
                    block_created: Default::default(),
                })
            })
            .enumerate()
            .map(|(i, coin)| {
                let utxo_id = UtxoId::new(Bytes32::from([0u8; 32]), i as u8);
                (utxo_id, coin)
            })
            .collect();
        for (id, coin) in coins {
            Storage::<UtxoId, Coin>::insert(&mut db, &id, &coin).unwrap();
        }

        db
    }

    #[test]
    fn largest_first_output() {
        // Setup
        let owner = Address::default();
        let colors = [Color::new([1u8; 32]), Color::new([2u8; 32])];
        let db = gen_test_db(owner, &colors);
        let query = |spend_query: &[SpendQueryElement],
                     max_inputs: u8|
         -> Result<Vec<(Color, u64)>, CoinQueryError> {
            let coins = largest_first(&db, spend_query, max_inputs);

            // Transform result for convenience
            coins.map(|coins| {
                coins
                    .into_iter()
                    .map(|coin| (coin.1.color, coin.1.amount))
                    .collect()
            })
        };

        // Query some amounts, including higher than the owner's balance
        for amount in 0..20 {
            let coins = query(&[(owner, colors[0], amount)], u8::MAX);

            // Transform result for convenience
            let coins = coins.map(|coins| {
                coins
                    .into_iter()
                    .map(|(color, amount)| {
                        // Check the color before we drop it
                        assert_eq!(color, colors[0]);

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

        // Query multiple colors
        let coins = query(&[(owner, colors[0], 3), (owner, colors[1], 6)], u8::MAX);
        assert_matches!(coins, Ok(coins) if coins == vec![(colors[0], 5), (colors[1], 5), (colors[1], 4)]);

        // Query with duplicate (owner, color)
        let coins = query(&[(owner, colors[0], 3), (owner, colors[0], 3)], u8::MAX);
        assert_matches!(coins, Ok(coins) if coins == vec![(colors[0], 5),  (colors[0], 4)]);

        // Query with too small max_inputs
        let coins = query(&[(owner, colors[0], 6)], 1);
        assert_matches!(coins, Err(CoinQueryError::NotEnoughInputs));
    }

    #[test]
    fn random_improve_output() {
        // Setup
        let owner = Address::default();
        let colors = [Color::new([1u8; 32]), Color::new([2u8; 32])];
        let db = gen_test_db(owner, &colors);
        let query = |spend_query: &[SpendQueryElement],
                     max_inputs: u8|
         -> Result<Vec<(Color, u64)>, CoinQueryError> {
            let coins = random_improve(&db, spend_query, max_inputs);

            // Transform result for convenience
            coins.map(|coins| {
                coins
                    .into_iter()
                    .map(|coin| (coin.1.color, coin.1.amount))
                    .sorted_by_key(|(color, amount)| {
                        (
                            colors.iter().position(|c| c == color).unwrap(),
                            Reverse(*amount),
                        )
                    })
                    .collect()
            })
        };

        // Query some amounts, including higher than the owner's balance
        for amount in 0..20 {
            let coins = query(&[(owner, colors[0], amount)], u8::MAX);

            // Transform result for convenience
            let coins = coins.map(|coins| {
                coins
                    .into_iter()
                    .map(|(color, amount)| {
                        // Check the color before we drop it
                        assert_eq!(color, colors[0]);

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

        // Query multiple colors
        let coins = query(&[(owner, colors[0], 3), (owner, colors[1], 6)], 3);
        assert_matches!(coins, Ok(ref coins) if coins.len() == 3);
        let coins = coins.unwrap();
        assert!(
            coins
                .iter()
                .filter(|c| c.0 == colors[0])
                .map(|c| c.1)
                .sum::<u64>()
                >= 3
        );
        assert!(
            coins
                .iter()
                .filter(|c| c.0 == colors[1])
                .map(|c| c.1)
                .sum::<u64>()
                >= 6
        );

        // Query with duplicate (owner, color)
        let coins = query(&[(owner, colors[0], 3), (owner, colors[0], 3)], u8::MAX);
        assert_matches!(coins, Ok(coins) if coins
            .iter()
            .filter(|c| c.0 == colors[0])
            .map(|c| c.1)
            .sum::<u64>()
            >= 6);

        // Query with too small max_inputs
        let coins = query(&[(owner, colors[0], 6)], 1);
        assert_matches!(coins, Err(CoinQueryError::NotEnoughInputs));
    }
}
