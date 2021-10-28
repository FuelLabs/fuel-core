use crate::{
    database::{columns::CONTRACTS, Database},
    state::Error,
};
use fuel_vm::prelude::{Contract, ContractId, Storage};
use std::borrow::Cow;

impl Storage<ContractId, Contract> for Database {
    type Error = Error;

    fn insert(&mut self, key: &ContractId, value: &Contract) -> Result<Option<Contract>, Error> {
        Database::insert(self, key.as_ref(), CONTRACTS, value.clone())
    }

    fn remove(&mut self, key: &ContractId) -> Result<Option<Contract>, Error> {
        Database::remove(self, key.as_ref(), CONTRACTS)
    }

    fn get(&self, key: &ContractId) -> Result<Option<Cow<Contract>>, Error> {
        self.get(key.as_ref(), CONTRACTS)
    }

    fn contains_key(&self, key: &ContractId) -> Result<bool, Error> {
        self.exists(key.as_ref(), CONTRACTS)
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
                .unwrap()
                .into_owned(),
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
