use crate::coin_query::{random_improve, SpendQueryElement};
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
use fuel_tx::consts::MAX_INPUTS;
use fuel_tx::{Address, AssetId as FuelTxAssetId, UtxoId};
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

    async fn asset_id(&self) -> HexString256 {
        self.1.asset_id.into()
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
    /// asset ID of the coins
    asset_id: Option<HexString256>,
}

#[derive(InputObject)]
struct SpendQueryElementInput {
    /// asset ID of the coins
    asset_id: HexString256,
    /// address of the owner
    amount: U64,
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

                // filter coins by asset ID
                let mut coins = coins;
                if let Some(asset_id) = filter.asset_id {
                    coins.retain(|coin| coin.1.asset_id == asset_id.0.into());
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

    async fn coins_to_spend(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "The Address of the utxo owner")] owner: HexString256,
        #[graphql(desc = "The total amount of each asset type to spend")] spend_query: Vec<
            SpendQueryElementInput,
        >,
        #[graphql(desc = "The max number of utxos that can be used")] max_inputs: Option<u8>,
    ) -> async_graphql::Result<Vec<Coin>> {
        let owner: Address = owner.0.into();
        let spend_query: Vec<SpendQueryElement> = spend_query
            .iter()
            .map(|e| (owner, FuelTxAssetId::from(e.asset_id.0), e.amount.0))
            .collect();
        let max_inputs: u8 = max_inputs.unwrap_or(MAX_INPUTS);

        let db = ctx.data_unchecked::<Database>();

        let coins = random_improve(db, &spend_query, max_inputs)?
            .into_iter()
            .map(|(id, coin)| Coin(id, coin))
            .collect();

        Ok(coins)
    }
}
