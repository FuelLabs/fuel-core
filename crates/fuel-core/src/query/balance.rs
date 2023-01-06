use crate::{
    database::{
        resource::{
            AssetQuery,
            AssetSpendTarget,
            AssetsQuery,
        },
        Database,
    },
    state::IterDirection,
};
use fuel_core_storage::Result as StorageResult;
use fuel_core_types::fuel_tx::{
    Address,
    AssetId,
};
use itertools::Itertools;
use std::{
    cmp::Ordering,
    collections::HashMap,
};

pub trait BalanceQueryData {
    type Item<'a>
    where
        Self: 'a;
    type Iter<'a>: Iterator<Item = Self::Item<'a>>
    where
        Self: 'a;

    fn balance(
        &self,
        owner: Address,
        asset_id: AssetId,
    ) -> StorageResult<(Address, u64, AssetId)>;

    fn balances(
        &self,
        filter: Address, // owner
        direction: IterDirection,
    ) -> StorageResult<Self::Iter<'_>>;
}

pub struct BalanceQueryContext<'a>(pub &'a Database);

impl BalanceQueryData for BalanceQueryContext<'_> {
    type Item<'a> = StorageResult<(Address, u64, AssetId)> where Self: 'a;
    type Iter<'a> = Box< dyn Iterator<Item = StorageResult<(Address, u64, AssetId)>> + 'a> where Self: 'a;

    fn balance(
        &self,
        owner: Address,
        asset_id: AssetId,
    ) -> StorageResult<(Address, u64, AssetId)> {
        let db = self.0;
        let balance = AssetQuery::new(
            &owner,
            &AssetSpendTarget::new(asset_id, u64::MAX, u64::MAX),
            None,
            db,
        )
        .unspent_resources()
        .map(|res| res.map(|resource| *resource.amount()))
        .try_fold(0u64, |mut balance, res| -> StorageResult<_> {
            let amount = res?;

            // Increase the balance
            balance += amount;

            Ok(balance)
        })?;

        Ok((owner, balance, asset_id))
    }

    fn balances(
        &self,
        filter: Address, // owner
        direction: IterDirection,
    ) -> StorageResult<Self::Iter<'_>> {
        let db = self.0;

        let owner = filter;

        let mut amounts_per_asset = HashMap::new();

        for resource in AssetsQuery::new(&owner, None, None, db).unspent_resources() {
            let resource = resource?;
            *amounts_per_asset.entry(*resource.asset_id()).or_default() +=
                resource.amount();
        }

        let mut balances = amounts_per_asset
            .into_iter()
            .map(|(asset_id, amount)| (owner, amount, asset_id))
            .collect_vec();

        balances.sort_by(|l, r| {
            if l.2 < r.2 {
                Ordering::Less
            } else {
                Ordering::Greater
            }
        });

        if direction == IterDirection::Reverse {
            balances.reverse();
        }

        let balances = balances.into_iter();

        Ok(Box::new(balances))
    }
}
