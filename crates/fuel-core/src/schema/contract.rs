use crate::{
    fuel_core_graphql_api::{
        IntoApiResult,
        QUERY_COSTS,
    },
    query::ContractQueryData,
    schema::{
        scalars::{
            AssetId,
            ContractId,
            HexString,
            Salt,
            U64,
        },
        ReadViewProvider,
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
use fuel_core_storage::{
    not_found,
    tables::ContractsRawCode,
};
use fuel_core_types::{
    fuel_types,
    services::graphql_api,
};

pub struct Contract(pub(crate) fuel_types::ContractId);

impl From<fuel_types::ContractId> for Contract {
    fn from(id: fuel_types::ContractId) -> Self {
        Self(id)
    }
}

#[Object]
impl Contract {
    async fn id(&self) -> ContractId {
        self.0.into()
    }

    #[graphql(complexity = "QUERY_COSTS.bytecode_read")]
    async fn bytecode(&self, ctx: &Context<'_>) -> async_graphql::Result<HexString> {
        let query = ctx.read_view()?;
        query
            .contract_bytecode(self.0)
            .map(HexString)
            .map_err(Into::into)
    }

    #[graphql(complexity = "QUERY_COSTS.storage_read")]
    async fn salt(&self, ctx: &Context<'_>) -> async_graphql::Result<Salt> {
        let query = ctx.read_view()?;
        query
            .contract_salt(self.0)
            .map(Into::into)
            .map_err(Into::into)
    }
}

#[derive(Default)]
pub struct ContractQuery;

#[Object]
impl ContractQuery {
    #[graphql(complexity = "QUERY_COSTS.storage_read")]
    async fn contract(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "ID of the Contract")] id: ContractId,
    ) -> async_graphql::Result<Option<Contract>> {
        let query = ctx.read_view()?;
        query
            .contract_exists(id.0)
            .and_then(|contract_exists| {
                if contract_exists {
                    Ok(id.0)
                } else {
                    Err(not_found!(ContractsRawCode))
                }
            })
            .into_api_result()
    }
}

pub struct ContractBalance(graphql_api::ContractBalance);

#[Object]
impl ContractBalance {
    async fn contract(&self) -> ContractId {
        self.0.owner.into()
    }

    async fn amount(&self) -> U64 {
        self.0.amount.into()
    }

    async fn asset_id(&self) -> AssetId {
        self.0.asset_id.into()
    }
}

#[derive(InputObject)]
struct ContractBalanceFilterInput {
    /// Filter assets based on the `contractId` field
    contract: ContractId,
}

#[derive(Default)]
pub struct ContractBalanceQuery;

#[Object]
impl ContractBalanceQuery {
    #[graphql(complexity = "QUERY_COSTS.storage_read")]
    async fn contract_balance(
        &self,
        ctx: &Context<'_>,
        contract: ContractId,
        asset: AssetId,
    ) -> async_graphql::Result<ContractBalance> {
        let contract_id = contract.into();
        let asset_id = asset.into();
        let query = ctx.read_view()?;
        query
            .contract_balance(contract_id, asset_id)
            .into_api_result()
            .map(|result| {
                result.unwrap_or_else(|| {
                    graphql_api::ContractBalance {
                        owner: contract_id,
                        amount: 0,
                        asset_id,
                    }
                    .into()
                })
            })
    }

    #[graphql(complexity = "{\
        QUERY_COSTS.storage_iterator\
        + (QUERY_COSTS.storage_read + first.unwrap_or_default() as usize) * child_complexity \
        + (QUERY_COSTS.storage_read + last.unwrap_or_default() as usize) * child_complexity\
    }")]
    async fn contract_balances(
        &self,
        ctx: &Context<'_>,
        filter: ContractBalanceFilterInput,
        first: Option<i32>,
        after: Option<String>,
        last: Option<i32>,
        before: Option<String>,
    ) -> async_graphql::Result<
        Connection<AssetId, ContractBalance, EmptyFields, EmptyFields>,
    > {
        let query = ctx.read_view()?;

        crate::schema::query_pagination(after, before, first, last, |start, direction| {
            let balances = query
                .contract_balances(
                    filter.contract.into(),
                    (*start).map(Into::into),
                    direction,
                )
                .map(move |balance| {
                    let balance = balance?;
                    let asset_id = balance.asset_id;

                    Ok((asset_id.into(), balance.into()))
                });

            Ok(balances)
        })
        .await
    }
}

impl From<graphql_api::ContractBalance> for ContractBalance {
    fn from(balance: graphql_api::ContractBalance) -> Self {
        ContractBalance(balance)
    }
}
