use crate::database::{
    storage::DatabaseColumn,
    Column,
};
use fuel_core_storage::tables::ContractsInfo;

impl DatabaseColumn for ContractsInfo {
    fn column() -> Column {
        Column::ContractsInfo
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::Database;
    use fuel_core_storage::StorageAsMut;
    use fuel_core_types::{
        fuel_types::{
            Bytes32,
            ContractId,
            Salt,
        },
        fuel_vm::Contract,
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
