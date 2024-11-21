use std::{
    cmp::Ordering,
    collections::HashMap,
    future,
};

use crate::{
    fuel_core_graphql_api::database::ReadView,
    graphql_api::storage::balances::TotalBalanceAmount,
};
use asset_query::{
    AssetQuery,
    AssetSpendTarget,
    AssetsQuery,
};
use fuel_core_services::yield_stream::StreamYieldExt;
use fuel_core_storage::{
    iter::IterDirection,
    Result as StorageResult,
};
use fuel_core_types::{
    fuel_tx::{
        Address,
        AssetId,
    },
    services::graphql_api::AddressBalance,
};
use futures::{
    stream,
    FutureExt,
    Stream,
    StreamExt,
    TryStreamExt,
};
use tracing::{
    debug,
    error,
};

pub mod asset_query;

impl ReadView {
    pub async fn balance(
        &self,
        owner: Address,
        asset_id: AssetId,
        base_asset_id: AssetId,
    ) -> StorageResult<AddressBalance> {
        let amount = if self.balances_enabled {
            debug!(%owner, %asset_id, "Querying balance with balances cache");
            self.off_chain.balance(&owner, &asset_id, &base_asset_id)?
        } else {
            debug!(%owner, %asset_id, "Querying balance without balances cache");
            AssetQuery::new(
                &owner,
                &AssetSpendTarget::new(asset_id, u64::MAX, u16::MAX),
                &base_asset_id,
                None,
                self,
            )
            .coins()
            .map(|res| res.map(|coins| coins.amount()))
            .try_fold(0u128, |balance, amount| async move {
                Ok(balance.checked_add(amount as u128).unwrap_or_else(|| {
                    // TODO[RC]: Balances overflow to be correctly handled. See: https://github.com/FuelLabs/fuel-core/issues/2428
                    error!(
                        %asset_id,
                        prev_balance=%balance,
                        "unable to change balance due to overflow");
                    u128::MAX
                }))
            })
            .await? as TotalBalanceAmount
        };

        Ok(AddressBalance {
            owner,
            amount,
            asset_id,
        })
    }

    pub fn balances<'a>(
        &'a self,
        owner: &'a Address,
        direction: IterDirection,
        base_asset_id: &'a AssetId,
    ) -> impl Stream<Item = StorageResult<AddressBalance>> + 'a {
        if self.balances_enabled {
            futures::future::Either::Left(self.balances_with_cache(
                owner,
                base_asset_id,
                direction,
            ))
        } else {
            futures::future::Either::Right(self.balances_without_cache(
                owner,
                base_asset_id,
                direction,
            ))
        }
    }

    fn balances_without_cache<'a>(
        &'a self,
        owner: &'a Address,
        base_asset_id: &'a AssetId,
        direction: IterDirection,
    ) -> impl Stream<Item = StorageResult<AddressBalance>> + 'a {
        debug!(%owner, "Querying balances without balances cache");
        let query = AssetsQuery::new(owner, None, None, self, base_asset_id);
        let stream = query.coins();

        stream
            .try_fold(
                HashMap::new(),
                move |mut amounts_per_asset, coin| async move {
                    let amount: &mut TotalBalanceAmount = amounts_per_asset
                        .entry(*coin.asset_id(base_asset_id))
                        .or_default();
                    let new_amount =
                        amount.saturating_add(coin.amount() as TotalBalanceAmount);
                    *amount = new_amount;
                    Ok(amounts_per_asset)
                },
            )
            .into_stream()
            .try_filter_map(move |amounts_per_asset| async move {
                let mut balances = amounts_per_asset
                    .into_iter()
                    .map(|(asset_id, amount)| AddressBalance {
                        owner: *owner,
                        amount,
                        asset_id,
                    })
                    .collect::<Vec<_>>();

                balances.sort_by(|l, r| {
                    if l.asset_id < r.asset_id {
                        Ordering::Less
                    } else {
                        Ordering::Greater
                    }
                });

                if direction == IterDirection::Reverse {
                    balances.reverse();
                }

                Ok(Some(futures::stream::iter(balances)))
            })
            .map_ok(|stream| stream.map(Ok))
            .try_flatten()
            .yield_each(self.batch_size)
    }

    fn balances_with_cache<'a>(
        &'a self,
        owner: &'a Address,
        base_asset_id: &AssetId,
        direction: IterDirection,
    ) -> impl Stream<Item = StorageResult<AddressBalance>> + 'a {
        debug!(%owner, "Querying balances using balances cache");
        match self.off_chain.balances(owner, base_asset_id) {
            Ok(balances) => {
                let iter = if direction == IterDirection::Reverse {
                    itertools::Either::Left(balances.into_iter().rev())
                } else {
                    itertools::Either::Right(balances.into_iter())
                };
                stream::iter(iter.map(|(asset_id, amount)| AddressBalance {
                    owner: *owner,
                    amount,
                    asset_id,
                }))
                .map(Ok)
                .into_stream()
                .yield_each(self.batch_size)
                .left_stream()
            }
            Err(err) => stream::once(future::ready(Err(err))).right_stream(),
        }
    }
}
