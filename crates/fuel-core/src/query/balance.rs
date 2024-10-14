use crate::fuel_core_graphql_api::database::ReadView;
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

impl ReadView {
    pub fn balance(
        &self,
        owner: Address,
        asset_id: AssetId,
        base_asset_id: AssetId,
    ) -> StorageResult<AddressBalance> {
        let amount = AssetQuery::new(
            &owner,
            &AssetSpendTarget::new(asset_id, u64::MAX, usize::MAX),
            &base_asset_id,
            None,
            self,
        )
        .coins()
        .map(|res| res.map(|coins| coins.amount()))
        .try_fold(0u64, |mut balance, res| -> StorageResult<_> {
            let amount = res?;

            // Increase the balance
            balance = balance.saturating_add(amount);

            Ok(balance)
        })?;

        Ok(AddressBalance {
            owner,
            amount,
            asset_id,
        })
    }

    pub fn balances(
        &self,
        owner: Address,
        direction: IterDirection,
        base_asset_id: AssetId,
    ) -> BoxedIter<StorageResult<AddressBalance>> {
        let mut amounts_per_asset = HashMap::new();
        let mut errors = vec![];

        for coin in AssetsQuery::new(&owner, None, None, self, &base_asset_id).coins() {
            match coin {
                Ok(coin) => {
                    let amount: &mut u64 = amounts_per_asset
                        .entry(*coin.asset_id(&base_asset_id))
                        .or_default();
                    *amount = amount.saturating_add(coin.amount());
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
