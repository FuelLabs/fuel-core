use crate::{
    database::{
        Database,
        KvStoreError,
    },
    schema::scalars::{
        AssetId,
        ContractId,
        HexString,
        Salt,
        U64,
    },
    state::IterDirection,
};
use anyhow::anyhow;
use async_graphql::{
    connection::{
        query,
        Connection,
        Edge,
        EmptyFields,
    },
    Context,
    InputObject,
    Object,
};
use fuel_core_interfaces::{
    common::{
        fuel_storage::StorageAsRef,
        fuel_tx,
        fuel_types,
        fuel_vm,
    },
    db::ContractsRawCode,
};
use std::iter::IntoIterator;

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
            .ok_or(KvStoreError::NotFound)?
            .into_owned();
        Ok(HexString(contract.into()))
    }
    async fn salt(&self, ctx: &Context<'_>) -> async_graphql::Result<Salt> {
        let contract_id = self.0;

        let db = ctx.data_unchecked::<Database>().clone();
        let salt = fuel_vm::storage::InterpreterStorage::storage_contract_root(
            &db,
            &contract_id,
        )
        .unwrap()
        .expect("Contract does not exist");

        let cleaned_salt: Salt = salt.clone().0.into();

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

        let result =
            fuel_vm::storage::InterpreterStorage::merkle_contract_asset_id_balance(
                &db,
                &contract_id,
                &asset_id,
            );
        let balance = result.unwrap().unwrap_or_default();

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

        query(
            after,
            before,
            first,
            last,
            |after: Option<AssetId>, before: Option<AssetId>, first, last| {
                async move {
                    // Calculate direction of which to iterate through rocksdb
                    let (records_to_fetch, direction) = if let Some(first) = first {
                        (first, IterDirection::Forward)
                    } else if let Some(last) = last {
                        (last, IterDirection::Reverse)
                    } else {
                        (0, IterDirection::Forward)
                    };

                    if (first.is_some() && before.is_some())
                        || (after.is_some() && before.is_some())
                        || (last.is_some() && after.is_some())
                    {
                        return Err(anyhow!("Wrong argument combination"))
                    }

                    let after = after.map(fuel_tx::AssetId::from);
                    let before = before.map(fuel_tx::AssetId::from);

                    let start = if direction == IterDirection::Forward {
                        after
                    } else {
                        before
                    };

                    let mut balances_iter = db.contract_balances(
                        filter.contract.into(),
                        start,
                        Some(direction),
                    );

                    let mut started = None;
                    if start.is_some() {
                        started = balances_iter.next();
                    }

                    let mut balances = balances_iter
                        .take(records_to_fetch + 1)
                        .map(|balance| {
                            let balance = balance?;

                            Ok(ContractBalance {
                                contract: filter.contract.into(),
                                amount: balance.1,
                                asset_id: balance.0,
                            })
                        })
                        .collect::<Result<Vec<ContractBalance>, KvStoreError>>()?;

                    let has_next_page = balances.len() > records_to_fetch;

                    if has_next_page {
                        balances.pop();
                    }

                    if direction == IterDirection::Reverse {
                        balances.reverse();
                    }

                    let mut connection =
                        Connection::new(started.is_some(), has_next_page);
                    connection.edges.extend(
                        balances
                            .into_iter()
                            .map(|item| Edge::new(item.asset_id.into(), item)),
                    );
                    Ok::<Connection<AssetId, ContractBalance>, anyhow::Error>(connection)
                }
            },
        )
        .await
    }
}
