use crate::fuel_core_graphql_api::database::ReadView;
use fuel_core_storage::{
    not_found,
    tables::{
        ContractsAssets,
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
    services::graphql_api::ContractBalance,
};

impl ReadView {
    pub fn contract_exists(&self, id: ContractId) -> StorageResult<bool> {
        self.on_chain
            .as_ref()
            .storage::<ContractsRawCode>()
            .contains_key(&id)
    }

    pub fn contract_bytecode(&self, id: ContractId) -> StorageResult<Vec<u8>> {
        let contract = self
            .on_chain
            .as_ref()
            .storage::<ContractsRawCode>()
            .get(&id)?
            .ok_or(not_found!(ContractsRawCode))?
            .into_owned();

        Ok(contract.into())
    }

    pub fn contract_balance(
        &self,
        contract_id: ContractId,
        asset_id: AssetId,
    ) -> StorageResult<ContractBalance> {
        let amount = self
            .on_chain
            .as_ref()
            .storage::<ContractsAssets>()
            .get(&(&contract_id, &asset_id).into())?
            .ok_or(not_found!(ContractsAssets))?
            .into_owned();

        Ok(ContractBalance {
            owner: contract_id,
            amount,
            asset_id,
        })
    }
}
