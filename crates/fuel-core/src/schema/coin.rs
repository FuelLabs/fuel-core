use crate::{
    database::Database,
    schema::scalars::{
        Address,
        AssetId,
        UtxoId,
        U64,
    },
};
use async_graphql::{
    connection::{
        Connection,
        EmptyFields,
    },
    Context,
    Enum,
    InputObject,
    Object,
};
use fuel_core_interfaces::{
    common::{
        fuel_storage::StorageAsRef,
        fuel_tx,
    },
    db::Coins,
    model::{
        Coin as CoinModel,
        CoinStatus as CoinStatusModel,
    },
    not_found,
};
use itertools::Itertools;

#[derive(Enum, Copy, Clone, Eq, PartialEq)]
#[graphql(remote = "CoinStatusModel")]
pub enum CoinStatus {
    Unspent,
    Spent,
}

pub struct Coin(pub(crate) fuel_tx::UtxoId, pub(crate) CoinModel);

#[Object]
impl Coin {
    async fn utxo_id(&self) -> UtxoId {
        self.0.into()
    }

    async fn owner(&self) -> Address {
        self.1.owner.into()
    }

    async fn amount(&self) -> U64 {
        self.1.amount.into()
    }

    async fn asset_id(&self) -> AssetId {
        self.1.asset_id.into()
    }

    async fn maturity(&self) -> U64 {
        self.1.maturity.into()
    }

    async fn status(&self) -> CoinStatus {
        self.1.status.into()
    }

    async fn block_created(&self) -> U64 {
        self.1.block_created.into()
    }
}

#[derive(InputObject)]
struct CoinFilterInput {
    /// Returns coins owned by the `owner`.
    owner: Address,
    /// Returns coins only with `asset_id`.
    asset_id: Option<AssetId>,
}

#[derive(Default)]
pub struct CoinQuery;

#[Object]
impl CoinQuery {
    /// Gets the coin by `utxo_id`.
    async fn coin(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "The ID of the coin")] utxo_id: UtxoId,
    ) -> async_graphql::Result<Option<Coin>> {
        let utxo_id = utxo_id.0;
        let db = ctx.data_unchecked::<Database>().clone();
        let block = db
            .storage::<Coins>()
            .get(&utxo_id)?
            .map(|coin| Coin(utxo_id, coin.into_owned()));
        Ok(block)
    }

    /// Gets all coins of some `owner` maybe filtered with by `asset_id` per page.
    /// It includes `CoinStatus::Spent` and `CoinStatus::Unspent` coins.
    async fn coins(
        &self,
        ctx: &Context<'_>,
        filter: CoinFilterInput,
        first: Option<i32>,
        after: Option<String>,
        last: Option<i32>,
        before: Option<String>,
    ) -> async_graphql::Result<Connection<UtxoId, Coin, EmptyFields, EmptyFields>> {
        crate::schema::query_pagination(after, before, first, last, |start, direction| {
            let db = ctx.data_unchecked::<Database>();
            let owner: fuel_tx::Address = filter.owner.into();
            let coin_ids: Vec<_> = db
                .owned_coins_ids(&owner, (*start).map(Into::into), Some(direction))
                .try_collect()?;
            let coins = coin_ids
                .into_iter()
                .map(|id| {
                    let value = db
                        .storage::<Coins>()
                        .get(&id)
                        .transpose()
                        .ok_or(not_found!(Coins))?
                        .map(|coin| Coin(id, coin.into_owned()))?;
                    let utxo_id: UtxoId = id.into();

                    Ok((utxo_id, value))
                })
                .filter_map(|result| {
                    if let (Ok((_, coin)), Some(filter_asset_id)) =
                        (&result, &filter.asset_id)
                    {
                        if coin.1.asset_id != filter_asset_id.0 {
                            return None
                        }
                    }

                    Some(result)
                });

            Ok(coins)
        })
        .await
    }
}
