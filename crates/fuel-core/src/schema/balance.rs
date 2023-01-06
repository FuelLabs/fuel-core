use crate::{
    query::BalanceQueryContext,
    schema::scalars::{
        Address,
        AssetId,
        U64,
    },
};
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
    fuel_types,
    services::graphql_api,
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
        let data = BalanceQueryContext(ctx.data_unchecked());
        let balance = data.balance(owner.0, asset_id.0)?.into();
        Ok(balance)
    }

    // TODO: We can't paginate over `AssetId` because it is not unique.
    //  It should be replaced with `UtxoId`.
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
        let query = BalanceQueryContext(ctx.data_unchecked());
        crate::schema::query_pagination(after, before, first, last, |_, direction| {
            let owner = filter.owner.into();
            Ok(query.balances(owner, direction).map(|result| {
                result.map(|balance| (balance.asset_id.into(), balance.into()))
            }))
        })
        .await
    }
}

impl From<graphql_api::Balance> for Balance {
    fn from(value: graphql_api::Balance) -> Self {
        Balance {
            owner: value.owner,
            amount: value.amount,
            asset_id: value.asset_id,
        }
    }
}
