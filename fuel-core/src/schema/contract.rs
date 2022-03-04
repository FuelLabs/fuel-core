use crate::database::{Database, KvStoreError};
use crate::schema::scalars::ContractId;
use crate::schema::scalars::HexString;
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
