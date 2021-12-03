use crate::{
    database::{columns::CONTRACTS_CODE_ROOT, Database},
    state::Error,
};
use fuel_vm::prelude::{Bytes32, ContractId, Salt, Storage};
use std::borrow::Cow;

impl Storage<ContractId, (Salt, Bytes32)> for Database {
    type Error = Error;

    fn insert(
        &mut self,
        key: &ContractId,
        value: &(Salt, Bytes32),
    ) -> Result<Option<(Salt, Bytes32)>, Error> {
        Database::insert(self, key.as_ref(), CONTRACTS_CODE_ROOT, *value)
    }

    fn remove(&mut self, key: &ContractId) -> Result<Option<(Salt, Bytes32)>, Error> {
        Database::remove(self, key.as_ref(), CONTRACTS_CODE_ROOT)
    }

    fn get(&self, key: &ContractId) -> Result<Option<Cow<(Salt, Bytes32)>>, Error> {
        Database::get(self, key.as_ref(), CONTRACTS_CODE_ROOT)
    }

    fn contains_key(&self, key: &ContractId) -> Result<bool, Error> {
        Database::exists(self, key.as_ref(), CONTRACTS_CODE_ROOT)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fuel_vm::prelude::Contract;
    use rand::rngs::StdRng;
    use rand::{Rng, SeedableRng};

    #[test]
    fn get() {
        let rng = &mut StdRng::seed_from_u64(2322u64);
        let contract_id: ContractId = ContractId::from([1u8; 32]);
        let contract: Contract = Contract::from(vec![32u8]);
        let root = contract.root();
        let salt: Salt = rng.gen();

        let database = Database::default();
        database
            .insert(
                contract_id.as_ref().to_vec(),
                CONTRACTS_CODE_ROOT,
                (salt, root),
            )
            .unwrap();

        assert_eq!(
            Storage::<ContractId, (Salt, Bytes32)>::get(&database, &contract_id)
                .unwrap()
                .unwrap()
                .into_owned(),
            (salt, root)
        );
    }

    #[test]
    fn put() {
        let rng = &mut StdRng::seed_from_u64(2322u64);
        let contract_id: ContractId = ContractId::from([1u8; 32]);
        let contract: Contract = Contract::from(vec![32u8]);
        let root = contract.root();
        let salt: Salt = rng.gen();

        let mut database = Database::default();
        Storage::<ContractId, (Salt, Bytes32)>::insert(&mut database, &contract_id, &(salt, root))
            .unwrap();

        let returned: (Salt, Bytes32) = database
            .get(contract_id.as_ref(), CONTRACTS_CODE_ROOT)
            .unwrap()
            .unwrap();
        assert_eq!(returned, (salt, root));
    }

    #[test]
    fn remove() {
        let rng = &mut StdRng::seed_from_u64(2322u64);
        let contract_id: ContractId = ContractId::from([1u8; 32]);
        let contract: Contract = Contract::from(vec![32u8]);
        let root = contract.root();
        let salt: Salt = rng.gen();

        let mut database = Database::default();
        database
            .insert(
                contract_id.as_ref().to_vec(),
                CONTRACTS_CODE_ROOT,
                (salt, root),
            )
            .unwrap();

        Storage::<ContractId, (Salt, Bytes32)>::remove(&mut database, &contract_id).unwrap();

        assert!(!database
            .exists(contract_id.as_ref(), CONTRACTS_CODE_ROOT)
            .unwrap());
    }

    #[test]
    fn exists() {
        let rng = &mut StdRng::seed_from_u64(2322u64);
        let contract_id: ContractId = ContractId::from([1u8; 32]);
        let contract: Contract = Contract::from(vec![32u8]);
        let root = contract.root();
        let salt: Salt = rng.gen();

        let database = Database::default();
        database
            .insert(
                contract_id.as_ref().to_vec(),
                CONTRACTS_CODE_ROOT,
                (salt, root),
            )
            .unwrap();

        assert!(
            Storage::<ContractId, (Salt, Bytes32)>::contains_key(&database, &contract_id).unwrap()
        );
    }
}
