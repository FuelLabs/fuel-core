use crate::{
    database::Database,
    schema::scalars::{
        AssetId,
        ContractId,
        HexString,
        Salt,
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
use fuel_core_interfaces::{
    common::{
        fuel_storage::StorageAsRef,
        fuel_types,
    },
    db::{
        ContractsAssets,
        ContractsInfo,
        ContractsRawCode,
    },
    not_found,
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

    async fn bytecode(&self, ctx: &Context<'_>) -> async_graphql::Result<HexString> {
        let db = ctx.data_unchecked::<Database>().clone();
        let contract = db
            .storage::<ContractsRawCode>()
            .get(&self.0)?
            .ok_or(not_found!(ContractsRawCode))?
            .into_owned();
        Ok(HexString(contract.into()))
    }
    async fn salt(&self, ctx: &Context<'_>) -> async_graphql::Result<Salt> {
        let contract_id = self.0;

        let db = ctx.data_unchecked::<Database>().clone();
        let (salt, _) = db
            .storage::<ContractsInfo>()
            .get(&contract_id)?
            .ok_or_else(|| anyhow!("Contract does not exist"))?
            .into_owned();

        let cleaned_salt: Salt = salt.into();

        Ok(cleaned_salt)
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
        let id: fuel_types::ContractId = id.0;
        let db = ctx.data_unchecked::<Database>().clone();
        let contract_exists = db.storage::<ContractsRawCode>().contains_key(&id)?;
        if !contract_exists {
            return Ok(None)
        }
        let contract = Contract(id);
        Ok(Some(contract))
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
        let contract_id: fuel_types::ContractId = contract.0;

        let db = ctx.data_unchecked::<Database>().clone();

        let asset_id: fuel_types::AssetId = asset.into();

        let result = db
            .storage::<ContractsAssets>()
            .get(&(&contract_id, &asset_id))?;
        let balance = result.unwrap_or_default().into_owned();

        Ok(ContractBalance {
            contract: contract.into(),
            amount: balance,
            asset_id,
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
