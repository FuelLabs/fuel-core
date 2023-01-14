use crate::{
    graphql_api::service::Database,
    state::IterDirection,
};
use fuel_core_storage::{
    not_found,
    tables::{
        ContractsAssets,
        ContractsInfo,
        ContractsRawCode,
    },
    Result as StorageResult,
    StorageAsRef,
};
use fuel_core_types::{
    fuel_types::{
        AssetId,
        ContractId,
    },
    fuel_vm::Salt,
    services::graphql_api::ContractBalance,
};

pub struct ContractQueryContext<'a>(pub &'a Database);

impl ContractQueryContext<'_> {
    pub fn contract_id(&self, id: ContractId) -> StorageResult<ContractId> {
        let contract_exists = self
            .0
            .as_ref()
            .storage::<ContractsRawCode>()
            .contains_key(&id)?;
        if contract_exists {
            Ok(id)
        } else {
            Err(not_found!(ContractsRawCode))
        }
    }

    pub fn contract_bytecode(&self, id: ContractId) -> StorageResult<Vec<u8>> {
        let contract = self
            .0
            .as_ref()
            .storage::<ContractsRawCode>()
            .get(&id)?
            .ok_or(not_found!(ContractsRawCode))?
            .into_owned();

        Ok(contract.into())
    }

    pub fn contract_salt(&self, id: ContractId) -> StorageResult<Salt> {
        let (salt, _) = self
            .0
            .as_ref()
            .storage::<ContractsInfo>()
            .get(&id)?
            .ok_or(not_found!(ContractsInfo))?
            .into_owned();

        Ok(salt)
    }

    pub fn contract_balance(
        &self,
        contract_id: ContractId,
        asset_id: AssetId,
    ) -> StorageResult<ContractBalance> {
        let amount = self
            .0
            .as_ref()
            .storage::<ContractsAssets>()
            .get(&(&contract_id, &asset_id))?
            .ok_or(not_found!(ContractsAssets))?
            .into_owned();

        Ok(ContractBalance {
            owner: contract_id,
            amount,
            asset_id,
        })
    }

    pub fn contract_balances(
        &self,
        contract_id: ContractId,
        start_asset: Option<AssetId>,
        direction: IterDirection,
    ) -> impl Iterator<Item = StorageResult<ContractBalance>> + '_ {
        self.0
            .contract_balances(contract_id, start_asset, direction)
    }
}
