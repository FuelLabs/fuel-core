use crate::database::columns::CONTRACTS;
use crate::database::Database;
use fuel_vm::data::{DataError, MerkleStorage};
use fuel_vm::prelude::{Contract, ContractId, Storage};

impl Storage<ContractId, Contract> for Database {
    fn insert(
        &mut self,
        key: &ContractId,
        value: &Contract,
    ) -> Result<Option<Contract>, DataError> {
        Database::insert(self, key.as_ref(), CONTRACTS, value.clone()).map_err(Into::into)
    }

    fn remove(&mut self, key: &ContractId) -> Result<Option<Contract>, DataError> {
        Database::remove(self, key.as_ref(), CONTRACTS).map_err(Into::into)
    }

    fn get(&self, key: &ContractId) -> Result<Option<Contract>, DataError> {
        self.get(key.as_ref(), CONTRACTS).map_err(Into::into)
    }

    fn contains_key(&self, key: &ContractId) -> Result<bool, DataError> {
        self.exists(key.as_ref(), CONTRACTS).map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get() {
        let contract_id: ContractId = ContractId::from([1u8; 32]);
        let contract: Contract = Contract::from(vec![32u8]);

        let database = Database::default();

        database
            .insert(contract_id.as_ref().to_vec(), CONTRACTS, contract.clone())
            .unwrap();

        assert_eq!(
            Storage::<ContractId, Contract>::get(&database, &contract_id)
                .unwrap()
                .unwrap(),
            contract
        );
    }

    #[test]
    fn put() {
        let contract_id: ContractId = ContractId::from([1u8; 32]);
        let contract: Contract = Contract::from(vec![32u8]);

        let mut database = Database::default();
        Storage::<ContractId, Contract>::insert(&mut database, &contract_id, &contract.clone())
            .unwrap();

        let returned: Contract = database
            .get(contract_id.as_ref(), CONTRACTS)
            .unwrap()
            .unwrap();
        assert_eq!(returned, contract);
    }

    #[test]
    fn remove() {
        let contract_id: ContractId = ContractId::from([1u8; 32]);
        let contract: Contract = Contract::from(vec![32u8]);

        let mut database = Database::default();
        database
            .insert(contract_id.as_ref().to_vec(), CONTRACTS, contract.clone())
            .unwrap();

        Storage::<ContractId, Contract>::remove(&mut database, &contract_id).unwrap();

        assert!(!database.exists(contract_id.as_ref(), CONTRACTS).unwrap());
    }

    #[test]
    fn exists() {
        let contract_id: ContractId = ContractId::from([1u8; 32]);
        let contract: Contract = Contract::from(vec![32u8]);

        let database = Database::default();
        database
            .insert(contract_id.as_ref().to_vec(), CONTRACTS, contract.clone())
            .unwrap();

        assert!(Storage::<ContractId, Contract>::contains_key(&database, &contract_id).unwrap());
    }
}
