use crate::{
    database::{
        Column,
        Database,
    },
    state::Error,
};
use fuel_core_interfaces::{
    common::{
        fuel_storage::{
            StorageInspect,
            StorageMutate,
        },
        fuel_vm::prelude::{
            Bytes32,
            ContractId,
            Salt,
        },
    },
    db::ContractsInfo,
};
use std::borrow::Cow;

impl StorageInspect<ContractsInfo> for Database {
    type Error = Error;

    fn get(&self, key: &ContractId) -> Result<Option<Cow<(Salt, Bytes32)>>, Error> {
        Database::get(self, key.as_ref(), Column::ContractsInfo)
    }

    fn contains_key(&self, key: &ContractId) -> Result<bool, Error> {
        Database::exists(self, key.as_ref(), Column::ContractsInfo)
    }
}

impl StorageMutate<ContractsInfo> for Database {
    fn insert(
        &mut self,
        key: &ContractId,
        value: &(Salt, Bytes32),
    ) -> Result<Option<(Salt, Bytes32)>, Error> {
        Database::insert(self, key.as_ref(), Column::ContractsInfo, *value)
    }

    fn remove(&mut self, key: &ContractId) -> Result<Option<(Salt, Bytes32)>, Error> {
        Database::remove(self, key.as_ref(), Column::ContractsInfo)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fuel_core_interfaces::common::{
        fuel_storage::StorageAsMut,
        fuel_vm::prelude::Contract,
    };
    use rand::{
        rngs::StdRng,
        Rng,
        SeedableRng,
    };

    #[test]
    fn get() {
        let rng = &mut StdRng::seed_from_u64(2322u64);
        let contract_id: ContractId = ContractId::from([1u8; 32]);
        let contract: Contract = Contract::from(vec![32u8]);
        let root = contract.root();
        let salt: Salt = rng.gen();

        let database = &mut Database::default();
        database
            .storage::<ContractsInfo>()
            .insert(&contract_id, &(salt, root))
            .unwrap();

        assert_eq!(
            database
                .storage::<ContractsInfo>()
                .get(&contract_id)
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

        let database = &mut Database::default();
        database
            .storage::<ContractsInfo>()
            .insert(&contract_id, &(salt, root))
            .unwrap();

        let returned: (Salt, Bytes32) = *database
            .storage::<ContractsInfo>()
            .get(&contract_id)
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

        let database = &mut Database::default();
        database
            .storage::<ContractsInfo>()
            .insert(&contract_id, &(salt, root))
            .unwrap();

        database
            .storage::<ContractsInfo>()
            .remove(&contract_id)
            .unwrap();

        assert!(!database
            .exists(contract_id.as_ref(), Column::ContractsInfo)
            .unwrap());
    }

    #[test]
    fn exists() {
        let rng = &mut StdRng::seed_from_u64(2322u64);
        let contract_id: ContractId = ContractId::from([1u8; 32]);
        let contract: Contract = Contract::from(vec![32u8]);
        let root = contract.root();
        let salt: Salt = rng.gen();

        let database = &mut Database::default();
        database
            .storage::<ContractsInfo>()
            .insert(&contract_id, &(salt, root))
            .unwrap();

        assert!(database
            .storage::<ContractsInfo>()
            .contains_key(&contract_id)
            .unwrap());
    }
}
