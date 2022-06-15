use crate::database::{Database, KvStoreError};
use crate::model::{Coin as CoinModel, CoinStatus};
use crate::schema::scalars::{Address, AssetId, U64};
use crate::state::{Error, IterDirection};
use async_graphql::InputObject;
use async_graphql::{
    connection::{query, Connection, Edge, EmptyFields},
    Context, Object,
};
use fuel_core_interfaces::common::{fuel_storage::Storage, fuel_tx, fuel_types};
use itertools::Itertools;

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

        let balance = db
            .owned_coins(owner.into(), None, None)
            .map(|res| -> Result<_, Error> {
                let id = res?;
                Storage::<fuel_tx::UtxoId, CoinModel>::get(db, &id)
                    .transpose()
                    .ok_or(KvStoreError::NotFound)?
                    .map_err(Into::into)
            })
            .filter_ok(|coin| {
                coin.status == CoinStatus::Unspent && coin.asset_id == asset_id.into()
            })
            .try_fold(
                Balance {
                    owner: owner.into(),
                    amount: 0u64,
                    asset_id: asset_id.into(),
                },
                |mut balance, res| -> Result<_, Error> {
                    let coin = res?;

                    // Increase the balance
                    balance.amount += coin.amount;

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
    ) -> async_graphql::Result<Connection<AssetId, Balance, EmptyFields, EmptyFields>> {
        let db = ctx.data_unchecked::<Database>();

        let balances = db
            .owned_coins(filter.owner.into(), None, None)
            .map(|res| -> Result<_, Error> {
                let id = res?;
                Storage::<fuel_tx::UtxoId, CoinModel>::get(db, &id)
                    .transpose()
                    .ok_or(KvStoreError::NotFound)?
                    .map_err(Into::into)
            })
            .filter_ok(|coin| coin.status == CoinStatus::Unspent)
            .try_fold(
                vec![] as Vec<Balance>,
                |mut balances, res| -> Result<_, Error> {
                    let coin = res?;

                    // Get or create the balance for the asset
                    let balance = if let Some(i) =
                        balances.iter().position(|b| b.asset_id == coin.asset_id)
                    {
                        &mut balances[i]
                    } else {
                        let balance = Balance {
                            owner: filter.owner.into(),
                            amount: 0,
                            asset_id: coin.asset_id,
                        };
                        balances.push(balance);
                        balances.last_mut().unwrap()
                    };

                    // Increase the balance
                    balance.amount += coin.amount;

                    Ok(balances)
                },
            )?;

        query(
            after,
            before,
            first,
            last,
            |after: Option<AssetId>, before: Option<AssetId>, first, last| async move {
                let (records_to_fetch, direction) = if let Some(first) = first {
                    (first, IterDirection::Forward)
                } else if let Some(last) = last {
                    (last, IterDirection::Reverse)
                } else {
                    (0, IterDirection::Forward)
                };

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
                                return false;
                            }
                        }
                        true
                    })
                    .take(records_to_fetch);
                let mut balances: Vec<Balance> = balances.collect();
                if direction == IterDirection::Reverse {
                    balances.reverse();
                }

                let mut connection =
                    Connection::new(started.is_some(), records_to_fetch <= balances.len());

                connection.edges.extend(
                    balances
                        .into_iter()
                        .map(|item| Edge::new(item.asset_id.into(), item)),
                );

                Ok::<Connection<AssetId, Balance>, KvStoreError>(connection)
            },
        )
        .await
    }
}
