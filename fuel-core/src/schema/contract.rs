use crate::database::{Database, KvStoreError};
use crate::schema::scalars::{AssetId, ContractId, HexString, Salt, U64};
use crate::state::IterDirection;
use async_graphql::connection::{query, Connection, Edge, EmptyFields};
use async_graphql::InputObject;
use async_graphql::{Context, Object};
use fuel_storage::Storage;
use fuel_vm::prelude::Contract as FuelVmContract;
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
        let contract = Storage::<fuel_types::ContractId, FuelVmContract>::get(&db, &self.0)?
            .ok_or(KvStoreError::NotFound)?
            .into_owned();
        Ok(HexString(contract.into()))
    }
    async fn salt(&self, ctx: &Context<'_>) -> async_graphql::Result<Salt> {
        let contract_id = self.0;

        let db = ctx.data_unchecked::<Database>().clone();
        let salt = fuel_vm::storage::InterpreterStorage::storage_contract_root(&db, &contract_id)
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
        let contract_exists =
            Storage::<fuel_types::ContractId, FuelVmContract>::contains_key(&db, &id)?;
        if !contract_exists {
            return Ok(None);
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

        let result = fuel_vm::storage::InterpreterStorage::merkle_contract_asset_id_balance(
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
    ) -> async_graphql::Result<Connection<AssetId, ContractBalance, EmptyFields, EmptyFields>> {
        let db = ctx.data_unchecked::<Database>().clone();

        let balances: Vec<ContractBalance> = db
            .contract_balances(filter.contract.into(), None, None)
            .map(|balance| -> ContractBalance {
                let asset_id = balance.unwrap();

                let result =
                    fuel_vm::storage::InterpreterStorage::merkle_contract_asset_id_balance(
                        &db,
                        &filter.contract.into(),
                        &asset_id,
                    )
                    .unwrap();

                let amount = result.unwrap_or(0);

                ContractBalance {
                    contract: filter.contract.into(),
                    amount,
                    asset_id,
                }
            })
            .collect();

        query(
            after,
            before,
            first,
            last,
            |after: Option<AssetId>, before: Option<AssetId>, first, last| async move {
                let (records_to_fetch, direction) = if let Some(first) = first {
                    (first, IterDirection::Forward)
                } else if let Some(last) = last {
                    (last, IterDirection::Reverse)
                } else {
                    (0, IterDirection::Forward)
                };

                let after = after.map(fuel_tx::AssetId::from);
                let before = before.map(fuel_tx::AssetId::from);

                let start;
                let end;

                if direction == IterDirection::Forward {
                    start = after;
                    end = before;
                } else {
                    start = before;
                    end = after;
                }

                let mut balances = balances.into_iter();
                if direction == IterDirection::Reverse {
                    balances = balances.rev().collect::<Vec<ContractBalance>>().into_iter();
                }
                if let Some(start) = start {
                    balances = balances
                        .skip_while(|balance| balance.asset_id == start)
                        .collect::<Vec<ContractBalance>>()
                        .into_iter();
                }
                let mut started = None;
                if start.is_some() {
                    // skip initial result
                    started = balances.next();
                }

                // take desired amount of results
                let balances = balances
                    .take_while(|balance| {
                        // take until we've reached the end
                        if let Some(end) = end.as_ref() {
                            if balance.asset_id == *end {
                                return false;
                            }
                        }
                        true
                    })
                    .take(records_to_fetch);
                let mut balances: Vec<ContractBalance> = balances.collect();
                if direction == IterDirection::Reverse {
                    balances.reverse();
                }

                let mut connection =
                    Connection::new(started.is_some(), records_to_fetch <= balances.len());
                connection.edges.extend(
                    balances
                        .into_iter()
                        .map(|item| Edge::new(item.asset_id.into(), item)),
                );
                Ok::<Connection<AssetId, ContractBalance>, KvStoreError>(connection)
            },
        )
        .await
    }
}
