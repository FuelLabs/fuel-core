use crate::database::Database;
use anyhow::anyhow;
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
};

#[cfg_attr(test, mockall::automock)]
/// Trait that specifies all the data required by the output message query.
pub trait ContractQueryData {
    fn contract(&self, id: ContractId) -> StorageResult<Option<ContractId>>;

    fn contract_bytecode(&self, id: ContractId) -> StorageResult<Vec<u8>>;

    fn contract_salt(&self, id: ContractId) -> StorageResult<Salt>;

    fn contract_balance(
        &self,
        contract: ContractId,
        asset: AssetId,
    ) -> StorageResult<(ContractId, u64, AssetId)>;
}

pub struct ContractQueryContext<'a>(pub &'a Database);

impl ContractQueryData for ContractQueryContext<'_> {
    fn contract(&self, id: ContractId) -> StorageResult<Option<ContractId>> {
        let db = self.0;
        let contract_exists = db.storage::<ContractsRawCode>().contains_key(&id)?;
        if !contract_exists {
            return Ok(None)
        }

        Ok(Some(id))
    }

    fn contract_balance(
        &self,
        contract_id: ContractId,
        asset_id: AssetId,
    ) -> StorageResult<(ContractId, u64, AssetId)> {
        let db = self.0;

        let result = db
            .storage::<ContractsAssets>()
            .get(&(&contract_id, &asset_id))?;

        let balance = result.unwrap_or_default().into_owned();

        Ok((contract_id, balance, asset_id))
    }

    fn contract_bytecode(&self, id: ContractId) -> StorageResult<Vec<u8>> {
        let db = self.0;

        let contract = db
            .storage::<ContractsRawCode>()
            .get(&id)?
            .ok_or(not_found!(ContractsRawCode))?
            .into_owned();

        Ok(contract.into())
    }

    fn contract_salt(&self, id: ContractId) -> StorageResult<Salt> {
        let db = self.0;

        let (salt, _) = db
            .storage::<ContractsInfo>()
            .get(&id)?
            .ok_or_else(|| anyhow!("Contract does not exist"))?
            .into_owned();

        Ok(salt)
    }
}
