use fuel_core_storage::transactional::Changes;
use fuel_core_types::fuel_tx::ContractId;
use fxhash::FxHashMap;

#[derive(Debug, Clone, Default)]
pub struct ContractsChanges {
    contracts_changes: FxHashMap<ContractId, u64>,
    latest_index: u64,
    changes_storage: FxHashMap<u64, (Vec<ContractId>, Changes)>,
}

impl ContractsChanges {
    pub fn new() -> Self {
        Self {
            contracts_changes: FxHashMap::default(),
            changes_storage: FxHashMap::default(),
            latest_index: 0,
        }
    }

    pub fn add_changes(&mut self, contract_ids: &[ContractId], changes: Changes) {
        let index = self.latest_index;
        self.latest_index += 1;
        for contract_id in contract_ids {
            self.contracts_changes.insert(*contract_id, index);
        }
        self.changes_storage
            .insert(index, (contract_ids.to_vec(), changes));
    }

    pub fn extract_changes(
        &mut self,
        contract_id: &ContractId,
    ) -> Option<(Vec<ContractId>, Changes)> {
        let id = self.contracts_changes.remove(contract_id)?;
        let (contract_ids, changes) = self.changes_storage.remove(&id)?;
        for contract_id in contract_ids.iter() {
            self.contracts_changes.remove(contract_id);
        }
        Some((contract_ids, changes))
    }

    pub fn extract_all_contracts_changes(&mut self) -> Vec<Changes> {
        let mut changes = vec![];
        for id in 0..self.latest_index {
            if let Some((_, change)) = self.changes_storage.remove(&id) {
                changes.push(change);
            }
        }
        self.clear();
        changes
    }

    pub fn clear(&mut self) {
        self.contracts_changes.clear();
        self.changes_storage.clear();
        self.latest_index = 0;
    }
}
