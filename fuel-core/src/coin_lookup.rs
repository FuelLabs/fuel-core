use crate::database::{Database, KvStoreError};
use crate::model::coin::Coin as CoinModel;
use crate::state::{self};
use fuel_storage::Storage;
use fuel_tx::{Color, UtxoId};
use itertools::Itertools;
use rand::prelude::*;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoinLookupError {
    #[error("store error occured")]
    KvStoreError(KvStoreError),
    #[error("state error occured")]
    StateError(state::Error),
    #[error("enough coins could not be found")]
    NotEnoughCoins,
    #[error("not enough inputs")]
    NotEnoughInputs,
}

impl From<KvStoreError> for CoinLookupError {
    fn from(e: KvStoreError) -> Self {
        CoinLookupError::KvStoreError(e)
    }
}

impl From<state::Error> for CoinLookupError {
    fn from(e: state::Error) -> Self {
        CoinLookupError::StateError(e)
    }
}

pub fn largest_first(
    db: &Database,
    all_coin_ids: &[Vec<UtxoId>],
    spend_query: &[(Color, u64)],
    max_inputs: u8,
) -> Result<Vec<(UtxoId, CoinModel)>, CoinLookupError> {
    let mut coins: Vec<(UtxoId, CoinModel)> = vec![];

    for (index, (_color, amount)) in spend_query.iter().enumerate() {
        let coin_ids = &all_coin_ids[index];
        let mut collected_amount = 0u64;

        let mut sorted_coins: Vec<(&UtxoId, CoinModel)> = coin_ids
            .iter()
            .map(|id| {
                Storage::<UtxoId, CoinModel>::get(db, id)
                    .transpose()
                    .ok_or(KvStoreError::NotFound)?
                    .map(|coin| (id, coin.into_owned()))
            })
            .try_collect()?;

        sorted_coins.sort_by_key(|coin| coin.1.amount);

        for (id, coin) in sorted_coins {
            // Break if we don't need any more coins
            if collected_amount >= *amount {
                break;
            }

            // Error if we can't fit more coins
            if coins.len() >= max_inputs as usize {
                return Err(CoinLookupError::NotEnoughInputs);
            }

            // Add to list
            collected_amount += coin.amount;
            coins.push((*id, coin));
        }

        if collected_amount < *amount {
            return Err(CoinLookupError::NotEnoughCoins);
        }
    }

    Ok(coins)
}

// An implementation of the method described on: https://iohk.io/en/blog/posts/2018/07/03/self-organisation-in-coin-selection/
pub fn random_improve(
    db: &Database,
    all_coin_ids: &[Vec<UtxoId>],
    spend_query: &[(Color, u64)],
    max_inputs: u8,
) -> Result<Vec<(UtxoId, CoinModel)>, CoinLookupError> {
    let mut coins: Vec<(UtxoId, CoinModel)> = vec![];

    let mut all_coin_ids: Vec<Vec<UtxoId>> = all_coin_ids.to_vec();
    let mut collected_amounts: Vec<u64> = spend_query.iter().map(|_| 0).collect();

    // Collect enough coins to satisfy the spend query
    for (index, (_color, amount)) in spend_query.iter().enumerate() {
        let coin_ids = &mut all_coin_ids[index];
        let collected_amount = &mut collected_amounts[index];

        loop {
            // Break if we don't need any more coins
            if *collected_amount >= *amount {
                break;
            }

            // Error if we can't fit more coins
            if coins.len() >= max_inputs as usize {
                return Err(CoinLookupError::NotEnoughInputs);
            }

            // Error if we don't have more coins
            if coin_ids.is_empty() {
                return Err(CoinLookupError::NotEnoughCoins);
            }

            // Remove random ID from the list
            let i = (0..coin_ids.len()).choose(&mut thread_rng()).unwrap();
            let id = coin_ids.swap_remove(i);

            // Get coin
            let coin = Storage::<UtxoId, CoinModel>::get(db, &id)
                .transpose()
                .ok_or(KvStoreError::NotFound)?
                .map(|coin| (id, coin.into_owned()))?;

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
    for (index, (_color, amount)) in spend_query.iter().enumerate() {
        let coin_ids = &mut all_coin_ids[index];
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
            if coin_ids.is_empty() {
                break;
            }

            // Remove random ID from the list
            let i = (0..coin_ids.len()).choose(&mut thread_rng()).unwrap();
            let id = coin_ids.swap_remove(i);

            // Get coin
            let coin = Storage::<UtxoId, CoinModel>::get(db, &id)
                .transpose()
                .ok_or(KvStoreError::NotFound)?
                .map(|coin| (id, coin.into_owned()))?;

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
