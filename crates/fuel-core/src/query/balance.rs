use crate::fuel_core_graphql_api::database::ReadView;
use asset_query::{
    AssetQuery,
    AssetSpendTarget,
    AssetsQuery,
};
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
    FutureExt,
    Stream,
    StreamExt,
    TryStreamExt,
};
use std::{
    cmp::Ordering,
    collections::HashMap,
};

pub mod asset_query;

impl ReadView {
    pub async fn balance(
        &self,
        owner: Address,
        asset_id: AssetId,
        base_asset_id: AssetId,
    ) -> StorageResult<AddressBalance> {
        let amount = AssetQuery::new(
            &owner,
            &AssetSpendTarget::new(asset_id, u64::MAX, u16::MAX),
            &base_asset_id,
            None,
            self,
        )
        .coins()
        .map(|res| res.map(|coins| coins.amount()))
        .try_fold(0u64, |balance, amount| {
            async move {
                // Increase the balance
                Ok(balance.saturating_add(amount))
            }
        })
        .await?;

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
        let query = AssetsQuery::new(owner, None, None, self, base_asset_id);
        let stream = query.coins();

        stream
            .try_fold(
                HashMap::new(),
                move |mut amounts_per_asset, coin| async move {
                    let amount: &mut u64 = amounts_per_asset
                        .entry(*coin.asset_id(base_asset_id))
                        .or_default();
                    *amount = amount.saturating_add(coin.amount());
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
            .chunks(self.butch_size)
            .filter_map(|chunk| async move {
                // Give a chance to other tasks to run.
                tokio::task::yield_now().await;
                Some(futures::stream::iter(chunk))
            })
            .flatten()
    }
}
