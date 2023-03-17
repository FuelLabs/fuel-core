use crate::fuel_core_graphql_api::service::Database;
use asset_query::{
    AssetQuery,
    AssetSpendTarget,
    AssetsQuery,
};
use fuel_core_storage::{
    iter::{
        BoxedIter,
        IntoBoxedIter,
        IterDirection,
    },
    Result as StorageResult,
};
use fuel_core_types::{
    fuel_tx::{
        Address,
        AssetId,
    },
    services::graphql_api::AddressBalance,
};
use itertools::Itertools;
use std::{
    cmp::Ordering,
    collections::HashMap,
};

pub mod asset_query;

pub trait BalanceQueryData: Send + Sync {
    fn balance(&self, owner: Address, asset_id: AssetId)
        -> StorageResult<AddressBalance>;

    fn balances(
        &self,
        owner: Address,
        direction: IterDirection,
    ) -> BoxedIter<StorageResult<AddressBalance>>;
}

impl BalanceQueryData for Database {
    fn balance(
        &self,
        owner: Address,
        asset_id: AssetId,
    ) -> StorageResult<AddressBalance> {
        let amount = AssetQuery::new(
            &owner,
            &AssetSpendTarget::new(asset_id, u64::MAX, u64::MAX),
            None,
            self,
        )
        .resources()
        .map(|res| res.map(|resource| *resource.amount()))
        .try_fold(0u64, |mut balance, res| -> StorageResult<_> {
            let amount = res?;

            // Increase the balance
            balance += amount;

            Ok(balance)
        })?;

        Ok(AddressBalance {
            owner,
            amount,
            asset_id,
        })
    }

    fn balances(
        &self,
        owner: Address,
        direction: IterDirection,
    ) -> BoxedIter<StorageResult<AddressBalance>> {
        let mut amounts_per_asset = HashMap::new();
        let mut errors = vec![];

        for resource in AssetsQuery::new(&owner, None, None, self).resources() {
            match resource {
                Ok(resource) => {
                    *amounts_per_asset.entry(*resource.asset_id()).or_default() +=
                        resource.amount();
                }
                Err(err) => {
                    errors.push(err);
                }
            }
        }

        let mut balances = amounts_per_asset
            .into_iter()
            .map(|(asset_id, amount)| AddressBalance {
                owner,
                amount,
                asset_id,
            })
            .collect_vec();

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

        balances
            .into_iter()
            .map(Ok)
            .chain(errors.into_iter().map(Err))
            .into_boxed()
    }
}
