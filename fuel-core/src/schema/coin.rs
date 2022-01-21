use crate::database::{Database, KvStoreError};
use crate::model::coin::{Coin as CoinModel, CoinStatus};
use crate::schema::scalars::{HexString256, U64};
use crate::state::{self, IterDirection};
use async_graphql::InputObject;
use async_graphql::{
    connection::{query, Connection, Edge, EmptyFields},
    Context, Object,
};
use fuel_storage::Storage;
use fuel_tx::consts::MAX_INPUTS;
use fuel_tx::{Address, Color as FuelTxColor, UtxoId};
use itertools::Itertools;
use rand::prelude::*;
use thiserror::Error;

use super::scalars::HexStringUtxoId;

pub struct Coin(UtxoId, CoinModel);

#[Object]
impl Coin {
    async fn utxo_id(&self) -> HexStringUtxoId {
        self.0.into()
    }

    async fn owner(&self) -> HexString256 {
        self.1.owner.into()
    }

    async fn amount(&self) -> U64 {
        self.1.amount.into()
    }

    async fn color(&self) -> HexString256 {
        self.1.color.into()
    }

    async fn maturity(&self) -> U64 {
        self.1.maturity.into()
    }

    async fn status(&self) -> CoinStatus {
        self.1.status
    }

    async fn block_created(&self) -> U64 {
        self.1.block_created.into()
    }
}

#[derive(InputObject)]
struct CoinFilterInput {
    /// address of the owner
    owner: HexString256,
    /// color of the coins
    color: Option<HexString256>,
}

#[derive(InputObject)]
struct SpendQueryElementInput {
    /// color of the coins
    color: HexString256,
    /// address of the owner
    amount: U64,
}

#[derive(Default)]
pub struct CoinQuery;

#[Object]
impl CoinQuery {
    async fn coin(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "utxo_id of the coin")] utxo_id: HexStringUtxoId,
    ) -> async_graphql::Result<Option<Coin>> {
        let utxo_id = utxo_id.0;
        let db = ctx.data_unchecked::<Database>().clone();
        let block = Storage::<UtxoId, CoinModel>::get(&db, &utxo_id)?
            .map(|coin| Coin(utxo_id, coin.into_owned()));
        Ok(block)
    }

    async fn coins(
        &self,
        ctx: &Context<'_>,
        after: Option<String>,
        before: Option<String>,
        first: Option<i32>,
        last: Option<i32>,
        filter: CoinFilterInput,
    ) -> async_graphql::Result<Connection<HexStringUtxoId, Coin, EmptyFields, EmptyFields>> {
        let db = ctx.data_unchecked::<Database>();

        query(
            after,
            before,
            first,
            last,
            |after: Option<HexStringUtxoId>, before: Option<HexStringUtxoId>, first, last| async move {
                let (records_to_fetch, direction) = if let Some(first) = first {
                    (first, IterDirection::Forward)
                } else if let Some(last) = last {
                    (last, IterDirection::Reverse)
                } else {
                    (0, IterDirection::Forward)
                };

                let after = after.map(UtxoId::from);
                let before = before.map(UtxoId::from);

                let start;
                let end;

                if direction == IterDirection::Forward {
                    start = after;
                    end = before;
                } else {
                    start = before;
                    end = after;
                }

                let owner: Address = filter.owner.into();

                let mut coin_ids = db.owned_coins(owner, start, Some(direction));
                let mut started = None;
                if start.is_some() {
                    // skip initial result
                    started = coin_ids.next();
                }

                // take desired amount of results
                let coins = coin_ids
                    .take_while(|r| {
                        // take until we've reached the end
                        if let (Ok(t), Some(end)) = (r, end.as_ref()) {
                            if *t == *end {
                                return false;
                            }
                        }
                        true
                    })
                    .take(records_to_fetch);
                let mut coins: Vec<UtxoId> = coins.try_collect()?;
                if direction == IterDirection::Reverse {
                    coins.reverse();
                }

                // TODO: do a batch get instead
                let coins: Vec<Coin> = coins
                    .into_iter()
                    .map(|id| {
                        Storage::<UtxoId, CoinModel>::get(db, &id)
                            .transpose()
                            .ok_or(KvStoreError::NotFound)?
                            .map(|coin| Coin(id, coin.into_owned()))
                    })
                    .try_collect()?;

                // filter coins by color
                let mut coins = coins;
                if let Some(color) = filter.color {
                    coins.retain(|coin| coin.1.color == color.0.into());
                }

                // filter coins by status
                coins.retain(|coin| coin.1.status == CoinStatus::Unspent);

                let mut connection =
                    Connection::new(started.is_some(), records_to_fetch <= coins.len());
                connection.append(
                    coins
                        .into_iter()
                        .map(|item| Edge::new(HexStringUtxoId::from(item.0), item)),
                );
                Ok(connection)
            },
        )
        .await
    }

    async fn coins_to_spend(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "The Address of the utxo owner")] owner: HexString256,
        #[graphql(desc = "The total amount of each asset type to spend")] spend_query: Vec<
            SpendQueryElementInput,
        >,
        #[graphql(desc = "The max number of utxos that can be used")] max_inputs: Option<i32>,
    ) -> async_graphql::Result<Vec<Coin>> {
        let owner: Address = owner.0.into();
        let spend_query: Vec<(FuelTxColor, u64)> = spend_query
            .iter()
            .map(|e| (FuelTxColor::from(e.color.0), e.amount.0))
            .collect();
        let max_inputs: u8 = max_inputs
            .map(u8::try_from)
            .map(Result::unwrap)
            .unwrap_or(MAX_INPUTS);

        let db = ctx.data_unchecked::<Database>();

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

        let owned_coin_ids: Vec<Vec<UtxoId>> = spend_query
            .iter()
            .map(|(color, _)| {
                db.owned_coins_by_color(owner, *color, None, None)
                    .try_collect()
            })
            .try_collect()?;

        let largest_first =
            |spend_query: Vec<(FuelTxColor, u64)>| -> Result<Vec<Coin>, CoinLookupError> {
                let mut coins: Vec<Coin> = vec![];

                for (index, (_color, amount)) in spend_query.iter().enumerate() {
                    let coin_ids = &owned_coin_ids[index];
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
                        coins.push(Coin(*id, coin));
                    }

                    if collected_amount < *amount {
                        return Err(CoinLookupError::NotEnoughCoins);
                    }
                }

                Ok(coins)
            };

        // An implementation of the method described on: https://iohk.io/en/blog/posts/2018/07/03/self-organisation-in-coin-selection/
        let random_improve =
            |spend_query: Vec<(FuelTxColor, u64)>| -> Result<Vec<Coin>, CoinLookupError> {
                let mut coins: Vec<Coin> = vec![];

                let mut collected_amounts: Vec<u64> = spend_query.iter().map(|_| 0).collect();
                let mut owned_coin_ids: Vec<Vec<UtxoId>> = owned_coin_ids.clone();

                // Collect enough coins to satisfy the spend query
                for (index, (_color, amount)) in spend_query.iter().enumerate() {
                    let coin_ids = &mut owned_coin_ids[index];
                    let collected_amount = &mut collected_amounts[index];

                    loop {
                        // Break if we don't need any more coins
                        if *collected_amount >= *amount {
                            break;
                        }

                        // Fallback to largest-first if we can't fit more coins
                        if coins.len() >= max_inputs as usize {
                            return largest_first(spend_query);
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
                            .map(|coin| Coin(id, coin.into_owned()))?;

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
                    let coin_ids = &mut owned_coin_ids[index];
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
                            .map(|coin| Coin(id, coin.into_owned()))?;

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
            };

        let coins = random_improve(spend_query)?;

        Ok(coins)
    }
}
