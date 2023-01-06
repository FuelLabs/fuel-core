use crate::{
    fuel_core_graphql_api::service::Database,
    query::{
        ContractQueryContext,
        ContractQueryData,
    },
    schema::scalars::{
        AssetId,
        ContractId,
        HexString,
        Salt,
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
use fuel_core_types::fuel_types;

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

    async fn bytecode(&self, ctx: &Context<'_>) -> async_graphql::Result<HexString> {
        let data = ContractQueryContext(ctx.data_unchecked());
        let bytecode = data.contract_bytecode(self.0)?;

        Ok(HexString(bytecode.into()))
    }

    async fn salt(&self, ctx: &Context<'_>) -> async_graphql::Result<Salt> {
        let data = ContractQueryContext(ctx.data_unchecked());
        let salt = data.contract_salt(self.0)?;

        Ok(salt.into())
    }
}

#[derive(Default)]
pub struct ContractQuery;

#[Object]
impl ContractQuery {
    async fn contract(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "ID of the Contract")] id: ContractId,
    ) -> async_graphql::Result<Option<Contract>> {
        let data = ContractQueryContext(ctx.data_unchecked());
        let contract = data.contract(id.0)?;

        if let Some(id) = contract {
            // Guranteed to exist otherwise contract would be `None`
            Ok(Some(Contract::from(id)))
        } else {
            Ok(None)
        }
    }
}

pub struct ContractBalance {
    contract: fuel_types::ContractId,
    amount: u64,
    asset_id: fuel_types::AssetId,
}

#[Object]
impl ContractBalance {
    async fn contract(&self) -> ContractId {
        self.contract.into()
    }

    async fn amount(&self) -> U64 {
        self.amount.into()
    }

    async fn asset_id(&self) -> AssetId {
        self.asset_id.into()
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
    async fn contract_balance(
        &self,
        ctx: &Context<'_>,
        contract: ContractId,
        asset: AssetId,
    ) -> async_graphql::Result<ContractBalance> {
        let data = ContractQueryContext(ctx.data_unchecked());
        let balance = data.contract_balance(contract.0, asset.0)?;

        Ok(ContractBalance {
            contract: balance.0,
            amount: balance.1,
            asset_id: balance.2,
        })
    }

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
        let db = ctx.data_unchecked::<Database>().clone();
        crate::schema::query_pagination(after, before, first, last, |start, direction| {
            let balances = db
                .contract_balances(
                    filter.contract.into(),
                    (*start).map(Into::into),
                    Some(direction),
                )
                .map(move |balance| {
                    let balance = balance?;
                    let asset_id: AssetId = balance.0.into();

                    Ok((
                        asset_id,
                        ContractBalance {
                            contract: filter.contract.into(),
                            amount: balance.1,
                            asset_id: balance.0,
                        },
                    ))
                });

            Ok(balances)
        })
        .await
    }
}
