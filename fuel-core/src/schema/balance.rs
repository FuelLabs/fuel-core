use crate::{
    database::{
        resource::{
            AssetQuery,
            AssetSpendTarget,
            AssetsQuery,
        },
        Database,
    },
    schema::scalars::{
        Address,
        AssetId,
        U64,
    },
    state::{
        Error,
        IterDirection,
    },
};
use anyhow::anyhow;
use async_graphql::{
    connection::{
        query,
        Connection,
        Edge,
        EmptyFields,
    },
    Context,
    InputObject,
    Object,
};
use fuel_core_interfaces::common::{
    fuel_tx,
    fuel_types,
};
use itertools::Itertools;
use std::{
    cmp::Ordering,
    collections::HashMap,
};

pub struct Balance {
    owner: fuel_types::Address,
    amount: u64,
    asset_id: fuel_types::AssetId,
}

#[Object]
impl Balance {
    async fn owner(&self) -> Address {
        self.owner.into()
    }

    async fn amount(&self) -> U64 {
        self.amount.into()
    }

    async fn asset_id(&self) -> AssetId {
        self.asset_id.into()
    }
}

#[derive(InputObject)]
struct BalanceFilterInput {
    /// Filter coins based on the `owner` field
    owner: Address,
}

#[derive(Default)]
pub struct BalanceQuery;

#[Object]
impl BalanceQuery {
    async fn balance(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "address of the owner")] owner: Address,
        #[graphql(desc = "asset_id of the coin")] asset_id: AssetId,
    ) -> async_graphql::Result<Balance> {
        let db = ctx.data_unchecked::<Database>();
        let owner = owner.into();
        let asset_id = asset_id.into();

        let balance = AssetQuery::new(
            &owner,
            &AssetSpendTarget::new(asset_id, u64::MAX, u64::MAX),
            None,
            db,
        )
        .unspent_resources()
        .map(|res| res.map(|resource| *resource.amount()))
        .try_fold(
            Balance {
                owner,
                amount: 0u64,
                asset_id,
            },
            |mut balance, res| -> Result<_, Error> {
                let amount = res?;

                // Increase the balance
                balance.amount += amount;

                Ok(balance)
            },
        )?;

        Ok(balance)
    }

    async fn balances(
        &self,
        ctx: &Context<'_>,
        filter: BalanceFilterInput,
        first: Option<i32>,
        after: Option<String>,
        last: Option<i32>,
        before: Option<String>,
    ) -> async_graphql::Result<Connection<AssetId, Balance, EmptyFields, EmptyFields>>
    {
        let db = ctx.data_unchecked::<Database>();
        let owner = filter.owner.into();

        let mut amounts_per_asset = HashMap::new();

        for resource in AssetsQuery::new(&owner, None, None, db).unspent_resources() {
            let resource = resource?;
            *amounts_per_asset.entry(*resource.asset_id()).or_default() +=
                resource.amount();
        }

        let mut balances = amounts_per_asset
            .into_iter()
            .map(|(asset_id, amount)| Balance {
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

        query(
            after,
            before,
            first,
            last,
            |after: Option<AssetId>, before: Option<AssetId>, first, last| {
                async move {
                    let (records_to_fetch, direction) = if let Some(first) = first {
                        (first, IterDirection::Forward)
                    } else if let Some(last) = last {
                        (last, IterDirection::Reverse)
                    } else {
                        (0, IterDirection::Forward)
                    };

                    if (first.is_some() && before.is_some())
                        || (after.is_some() && before.is_some())
                        || (last.is_some() && after.is_some())
                    {
                        return Err(anyhow!("Wrong argument combination"))
                    }

                    let after = after.map(fuel_tx::AssetId::from);
                    let before = before.map(fuel_tx::AssetId::from);

                    let start;
                    let end;

                    if direction == IterDirection::Forward {
                        start = after;
                        end = before;
                    } else {
                        start = before;
                        end = after;
                    }

                    let mut balances = balances.into_iter();
                    if direction == IterDirection::Reverse {
                        balances = balances.rev().collect::<Vec<Balance>>().into_iter();
                    }
                    if let Some(start) = start {
                        balances = balances
                            .skip_while(|balance| balance.asset_id == start)
                            .collect::<Vec<Balance>>()
                            .into_iter();
                    }
                    let mut started = None;
                    if start.is_some() {
                        // skip initial result
                        started = balances.next();
                    }

                    // take desired amount of results
                    let balances = balances
                        .take_while(|balance| {
                            // take until we've reached the end
                            if let Some(end) = end.as_ref() {
                                if balance.asset_id == *end {
                                    return false
                                }
                            }
                            true
                        })
                        .take(records_to_fetch);
                    let mut balances: Vec<Balance> = balances.collect();
                    if direction == IterDirection::Reverse {
                        balances.reverse();
                    }

                    let mut connection = Connection::new(
                        started.is_some(),
                        records_to_fetch <= balances.len(),
                    );

                    connection.edges.extend(
                        balances
                            .into_iter()
                            .map(|item| Edge::new(item.asset_id.into(), item)),
                    );

                    Ok::<Connection<AssetId, Balance>, anyhow::Error>(connection)
                }
            },
        )
        .await
    }
}
