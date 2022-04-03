use crate::database::{Database, KvStoreError};
use crate::model::coin::{Coin as CoinModel, CoinStatus};
use crate::schema::scalars::{Address, AssetId, U64};
use crate::state::IterDirection;
use async_graphql::InputObject;
use async_graphql::{
    connection::{query, Connection, Edge, EmptyFields},
    Context, Object,
};
use fuel_storage::Storage;
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

        let coin_ids: Vec<fuel_tx::UtxoId> =
            db.owned_coins(owner.into(), None, None).try_collect()?;
        let mut coins: Vec<(fuel_tx::UtxoId, CoinModel)> = coin_ids
            .into_iter()
            .map(|id| {
                Storage::<fuel_tx::UtxoId, CoinModel>::get(db, &id)
                    .transpose()
                    .ok_or(KvStoreError::NotFound)?
                    .map(|coin| (id, coin.into_owned()))
            })
            .try_collect()?;
        coins.retain(|(_, coin)| coin.status == CoinStatus::Unspent);

        let sum_amount = coins
            .iter()
            .filter(|(_, coin)| coin.asset_id == asset_id.into())
            .map(|(_, coin)| coin.amount)
            .sum();

        Ok(Balance {
            owner: owner.into(),
            amount: sum_amount,
            asset_id: asset_id.into(),
        })
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

        let coin_ids: Vec<fuel_tx::UtxoId> = db
            .owned_coins(filter.owner.into(), None, None)
            .try_collect()?;
        let mut coins: Vec<(fuel_tx::UtxoId, CoinModel)> = coin_ids
            .into_iter()
            .map(|id| {
                Storage::<fuel_tx::UtxoId, CoinModel>::get(db, &id)
                    .transpose()
                    .ok_or(KvStoreError::NotFound)?
                    .map(|coin| (id, coin.into_owned()))
            })
            .try_collect()?;
        coins.retain(|(_, coin)| coin.status == CoinStatus::Unspent);

        let balances = coins
            .iter()
            .group_by(|(_, coin)| coin.asset_id)
            .into_iter()
            .map(|(asset_id, coins)| {
                let sum_amount = coins.map(|(_, coin)| coin.amount).sum();

                Balance {
                    owner: filter.owner.into(),
                    amount: sum_amount,
                    asset_id,
                }
            })
            .collect::<Vec<Balance>>();

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
                connection.append(
                    balances
                        .into_iter()
                        .map(|item| Edge::new(item.asset_id.into(), item)),
                );
                Ok(connection)
            },
        )
        .await
    }
}
