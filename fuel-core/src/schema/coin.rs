use crate::database::{Database, KvStoreError};
use crate::model::coin::{Coin as CoinModel, CoinStatus};
use crate::schema::scalars::{HexString256, U64};
use crate::state::IterDirection;
use async_graphql::InputObject;
use async_graphql::{
    connection::{query, Connection, Edge, EmptyFields},
    Context, Object,
};
use fuel_storage::Storage;
use fuel_tx::{Address, UtxoId};
use itertools::Itertools;

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
}
