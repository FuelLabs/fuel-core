use crate::database::{columns, KvStore, KvStoreError, SharedDatabase};
use crate::model::coin::{Coin as CoinModel, CoinStatus};
use crate::schema::scalars::{HexString, HexString256};
use crate::state::IterDirection;
use async_graphql::connection::{query, Connection, Edge, EmptyFields};
use async_graphql::{Context, Object};
use fuel_asm::Word;
use fuel_tx::{Address, Bytes32};
use itertools::Itertools;
use std::convert::TryInto;

pub struct Coin(Bytes32, CoinModel);

#[Object]
impl Coin {
    async fn id(&self) -> HexString256 {
        self.0.into()
    }

    async fn owner(&self) -> HexString256 {
        self.1.owner.into()
    }

    async fn amount(&self) -> Word {
        self.1.amount
    }

    async fn color(&self) -> HexString256 {
        self.1.color.into()
    }

    async fn maturity(&self) -> u32 {
        self.1.maturity.into()
    }

    async fn status(&self) -> CoinStatus {
        self.1.status.into()
    }

    async fn block_created(&self) -> u32 {
        self.1.block_created.into()
    }
}

#[derive(Default)]
pub struct CoinQuery;

#[Object]
impl CoinQuery {
    async fn coin(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "id of the coin")] id: HexString256,
    ) -> async_graphql::Result<Option<Coin>> {
        let id: Bytes32 = id.0.try_into()?;
        let db = ctx.data_unchecked::<SharedDatabase>().as_ref();
        let block = KvStore::<Bytes32, CoinModel>::get(db, &id)?.map(|coin| Coin(id, coin));
        Ok(block)
    }

    async fn coins_by_owner(
        &self,
        ctx: &Context<'_>,
        after: Option<String>,
        before: Option<String>,
        first: Option<i32>,
        last: Option<i32>,
        #[graphql(desc = "address of the owner")] owner: HexString256,
    ) -> async_graphql::Result<Connection<HexString256, Coin, EmptyFields, EmptyFields>> {
        let db = ctx.data_unchecked::<SharedDatabase>().as_ref();

        query(
            after,
            before,
            first,
            last,
            |after: Option<HexString256>, before: Option<HexString256>, first, last| async move {
                let (records_to_fetch, direction) = if let Some(first) = first {
                    (first, IterDirection::Forward)
                } else if let Some(last) = last {
                    (last, IterDirection::Reverse)
                } else {
                    (0, IterDirection::Forward)
                };

                let after = after.map(|s| Bytes32::from(s));
                let before = before.map(|s| Bytes32::from(s));

                let start;
                let end;

                if direction == IterDirection::Forward {
                    start = after;
                    end = before;
                } else {
                    start = before;
                    end = after;
                }

                let owner: Address = owner.into();

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
                        if let (Ok(t), Some(end)) = (r, end) {
                            if t == &end {
                                return false;
                            }
                        }
                        true
                    })
                    .take(records_to_fetch);
                let mut coins: Vec<Bytes32> = coins.try_collect()?;
                if direction == IterDirection::Reverse {
                    coins.reverse();
                }

                // TODO: do a batch get instead
                let coins: Vec<Coin> = coins
                    .into_iter()
                    .map(|id| {
                        KvStore::<Bytes32, CoinModel>::get(db, &id)
                            .transpose()
                            .ok_or(KvStoreError::NotFound)?
                            .map(|coin| Coin(id, coin))
                    })
                    .try_collect()?;

                let mut connection =
                    Connection::new(started.is_some(), records_to_fetch <= coins.len());
                connection.append(
                    coins
                        .into_iter()
                        .map(|item| Edge::new(HexString256::from(item.0), item)),
                );
                Ok(connection)
            },
        )
        .await
    }
}
