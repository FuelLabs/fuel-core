use crate::{
    fuel_core_graphql_api::{
        service::Database,
        IntoApiResult,
    },
    query::CoinQueryData,
    schema::scalars::{
        Address,
        AssetId,
        UtxoId,
        U64,
    },
};
use anyhow::anyhow;
use async_graphql::{
    connection::{
        Connection,
        EmptyFields,
    },
    Context,
    InputObject,
    Object,
};
use fuel_core_types::{
    entities::coin::Coin as CoinModel,
    fuel_tx,
};

pub struct Coin(pub(crate) CoinModel);

#[Object]
impl Coin {
    async fn utxo_id(&self) -> UtxoId {
        self.0.utxo_id.into()
    }

    async fn owner(&self) -> Address {
        self.0.owner.into()
    }

    async fn amount(&self) -> U64 {
        self.0.amount.into()
    }

    async fn asset_id(&self) -> AssetId {
        self.0.asset_id.into()
    }

    async fn maturity(&self) -> U64 {
        self.0.maturity.into()
    }

    /// TxPointer - the height of the block this coin was created in
    async fn block_created(&self) -> U64 {
        u64::from(self.0.tx_pointer.block_height()).into()
    }

    /// TxPointer - the index of the transaction that created this coin
    async fn tx_created_idx(&self) -> U64 {
        u64::from(self.0.tx_pointer.tx_index()).into()
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
        let data: &Database = ctx.data_unchecked();
        data.coin(utxo_id.0).into_api_result()
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
        // Rocksdb doesn't support reverse iteration over a prefix
        if matches!(last, Some(last) if last > 0) {
            return Err(
                anyhow!("reverse pagination isn't supported for this resource").into(),
            )
        }

        let query: &Database = ctx.data_unchecked();
        crate::schema::query_pagination(after, before, first, last, |start, direction| {
            let owner: fuel_tx::Address = filter.owner.into();
            let coins = query
                .owned_coins(&owner, (*start).map(Into::into), direction)
                .into_iter()
                .filter_map(|result| {
                    if let (Ok(coin), Some(filter_asset_id)) = (&result, &filter.asset_id)
                    {
                        if coin.asset_id != filter_asset_id.0 {
                            return None
                        }
                    }

                    Some(result)
                })
                .map(|res| res.map(|coin| (coin.utxo_id.into(), coin.into())));

            Ok(coins)
        })
        .await
    }
}

impl From<CoinModel> for Coin {
    fn from(value: CoinModel) -> Self {
        Coin(value)
    }
}
