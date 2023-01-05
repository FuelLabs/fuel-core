use crate::database::Database;
use fuel_core_storage::{
    tables::ContractsRawCode,
    Result as StorageResult,
    StorageAsRef,
};
use fuel_core_types::fuel_types::ContractId;

#[cfg_attr(test, mockall::automock)]
/// Trait that specifies all the data required by the output message query.
pub trait ContractQueryData {
    fn contract(&self, id: ContractId) -> StorageResult<Option<ContractId>>;
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
}
