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
    use fuel_core_storage::{
        ContractInfo,
        StorageAsMut,
    };
    use fuel_core_types::{
        fuel_types::ContractId,
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
        let info = ContractInfo {
            salt: rng.gen(),
            root: contract.root(),
        };

        let database = &mut Database::default();
        database
            .storage::<ContractsInfo>()
            .insert(&contract_id, &info)
            .unwrap();

        assert_eq!(
            database
                .storage::<ContractsInfo>()
                .get(&contract_id)
                .unwrap()
                .unwrap(),
            info
        );
    }

    #[test]
    fn put() {
        let rng = &mut StdRng::seed_from_u64(2322u64);
        let contract_id: ContractId = ContractId::from([1u8; 32]);
        let contract: Contract = Contract::from(vec![32u8]);
        let info = ContractInfo {
            salt: rng.gen(),
            root: contract.root(),
        };

        let database = &mut Database::default();
        database
            .storage::<ContractsInfo>()
            .insert(&contract_id, &info)
            .unwrap();

        let returned: ContractInfo = database
            .storage::<ContractsInfo>()
            .get(&contract_id)
            .unwrap()
            .unwrap();
        assert_eq!(returned, info);
    }

    #[test]
    fn remove() {
        let rng = &mut StdRng::seed_from_u64(2322u64);
        let contract_id: ContractId = ContractId::from([1u8; 32]);
        let contract: Contract = Contract::from(vec![32u8]);
        let info = ContractInfo {
            salt: rng.gen(),
            root: contract.root(),
        };

        let database = &mut Database::default();
        database
            .storage::<ContractsInfo>()
            .insert(&contract_id, &info)
            .unwrap();

        database
            .storage::<ContractsInfo>()
            .remove(&contract_id)
            .unwrap();

        assert!(!database
            .contains_key(contract_id.as_ref(), Column::ContractsInfo)
            .unwrap());
    }

    #[test]
    fn exists() {
        let rng = &mut StdRng::seed_from_u64(2322u64);
        let contract_id: ContractId = ContractId::from([1u8; 32]);
        let contract: Contract = Contract::from(vec![32u8]);
        let info = ContractInfo {
            salt: rng.gen(),
            root: contract.root(),
        };

        let database = &mut Database::default();
        database
            .storage::<ContractsInfo>()
            .insert(&contract_id, &info)
            .unwrap();

        assert!(database
            .storage::<ContractsInfo>()
            .contains_key(&contract_id)
            .unwrap());
    }
}
