use fuel_core_storage::{
    ContractsAssetKey,
    ContractsStateKey,
};
use fuel_core_types::fuel_types::ContractId;

use crate::database::KeyFilter;

#[derive(Debug, Clone, Copy)]
pub struct ByContractId {
    contract_id: ContractId,
}

impl ByContractId {
    pub fn new(contract_id: ContractId) -> Self {
        Self { contract_id }
    }
}

trait HasContractId {
    fn extract_contract_id(&self) -> ContractId;
}

impl<K: HasContractId> KeyFilter<K> for ByContractId {
    fn start_at_prefix(&self) -> Option<&[u8]> {
        Some(self.contract_id.as_ref())
    }
    fn should_stop(&self, key: &K) -> bool {
        key.extract_contract_id() != self.contract_id
    }
}

impl HasContractId for ContractId {
    fn extract_contract_id(&self) -> ContractId {
        *self
    }
}

impl HasContractId for ContractsAssetKey {
    fn extract_contract_id(&self) -> ContractId {
        *self.contract_id()
    }
}

impl HasContractId for ContractsStateKey {
    fn extract_contract_id(&self) -> ContractId {
        *self.contract_id()
    }
}
