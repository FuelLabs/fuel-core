use crate::{
    tables::{
        ContractsLatestUtxo,
        ContractsRawCode,
    },
    Column,
    Database,
    Error,
    IterDirection,
    MultiKey,
};
use fuel_core_interfaces::common::{
    fuel_storage::{
        StorageInspect,
        StorageMutate,
    },
    fuel_tx::UtxoId,
    fuel_types::Word,
    fuel_vm::prelude::{
        AssetId,
        Contract,
        ContractId,
    },
};
use std::borrow::Cow;

impl StorageInspect<ContractsRawCode> for Database {
    type Error = Error;

    fn get(&self, key: &ContractId) -> Result<Option<Cow<Contract>>, Error> {
        self.get(key.as_ref(), Column::ContractsRawCode)
    }

    fn contains_key(&self, key: &ContractId) -> Result<bool, Error> {
        self.exists(key.as_ref(), Column::ContractsRawCode)
    }
}

impl StorageMutate<ContractsRawCode> for Database {
    fn insert(
        &mut self,
        key: &ContractId,
        value: &[u8],
    ) -> Result<Option<Contract>, Error> {
        Database::insert(self, key.as_ref(), Column::ContractsRawCode, value)
    }

    fn remove(&mut self, key: &ContractId) -> Result<Option<Contract>, Error> {
        Database::remove(self, key.as_ref(), Column::ContractsRawCode)
    }
}

impl StorageInspect<ContractsLatestUtxo> for Database {
    type Error = Error;

    fn get(&self, key: &ContractId) -> Result<Option<Cow<UtxoId>>, Self::Error> {
        self.get(key.as_ref(), Column::ContractsLatestUtxo)
    }

    fn contains_key(&self, key: &ContractId) -> Result<bool, Self::Error> {
        self.exists(key.as_ref(), Column::ContractsLatestUtxo)
    }
}

impl StorageMutate<ContractsLatestUtxo> for Database {
    fn insert(
        &mut self,
        key: &ContractId,
        value: &UtxoId,
    ) -> Result<Option<UtxoId>, Self::Error> {
        Database::insert(self, key.as_ref(), Column::ContractsLatestUtxo, *value)
    }

    fn remove(&mut self, key: &ContractId) -> Result<Option<UtxoId>, Self::Error> {
        Database::remove(self, key.as_ref(), Column::ContractsLatestUtxo)
    }
}

impl Database {
    pub fn contract_balances(
        &self,
        contract: ContractId,
        start_asset: Option<AssetId>,
        direction: Option<IterDirection>,
    ) -> impl Iterator<Item = Result<(AssetId, Word), Error>> + '_ {
        self.iter_all::<Vec<u8>, Word>(
            Column::ContractsAssets,
            Some(contract.as_ref().to_vec()),
            start_asset
                .map(|asset_id| MultiKey::new(&(&contract, &asset_id)).as_ref().to_vec()),
            direction,
        )
        .map(|res| {
            res.map(|(key, balance)| {
                (AssetId::new(key[32..].try_into().unwrap()), balance)
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fuel_core_interfaces::common::{
        fuel_storage::StorageAsMut,
        fuel_tx::TxId,
    };

    #[test]
    fn raw_code_get() {
        let contract_id: ContractId = ContractId::from([1u8; 32]);
        let contract: Contract = Contract::from(vec![32u8]);

        let database = &mut Database::default();

        database
            .storage::<ContractsRawCode>()
            .insert(&contract_id, contract.as_ref())
            .unwrap();

        assert_eq!(
            database
                .storage::<ContractsRawCode>()
                .get(&contract_id)
                .unwrap()
                .unwrap()
                .into_owned(),
            contract
        );
    }

    #[test]
    fn raw_code_put() {
        let contract_id: ContractId = ContractId::from([1u8; 32]);
        let contract: Contract = Contract::from(vec![32u8]);

        let database = &mut Database::default();
        database
            .storage::<ContractsRawCode>()
            .insert(&contract_id, contract.as_ref())
            .unwrap();

        let returned: Contract = database
            .storage::<ContractsRawCode>()
            .get(&contract_id)
            .unwrap()
            .unwrap()
            .into_owned();
        assert_eq!(returned, contract);
    }

    #[test]
    fn raw_code_remove() {
        let contract_id: ContractId = ContractId::from([1u8; 32]);
        let contract: Contract = Contract::from(vec![32u8]);

        let database = &mut Database::default();
        database
            .storage::<ContractsRawCode>()
            .insert(&contract_id, contract.as_ref())
            .unwrap();

        database
            .storage::<ContractsRawCode>()
            .remove(&contract_id)
            .unwrap();

        assert!(!database
            .storage::<ContractsRawCode>()
            .contains_key(&contract_id)
            .unwrap());
    }

    #[test]
    fn raw_code_exists() {
        let contract_id: ContractId = ContractId::from([1u8; 32]);
        let contract: Contract = Contract::from(vec![32u8]);

        let database = &mut Database::default();
        database
            .storage::<ContractsRawCode>()
            .insert(&contract_id, contract.as_ref())
            .unwrap();

        assert!(database
            .storage::<ContractsRawCode>()
            .contains_key(&contract_id)
            .unwrap());
    }

    #[test]
    fn latest_utxo_get() {
        let contract_id: ContractId = ContractId::from([1u8; 32]);
        let utxo_id: UtxoId = UtxoId::new(TxId::new([2u8; 32]), 4);

        let database = &mut Database::default();

        database
            .storage::<ContractsLatestUtxo>()
            .insert(&contract_id, &utxo_id)
            .unwrap();

        assert_eq!(
            database
                .storage::<ContractsLatestUtxo>()
                .get(&contract_id)
                .unwrap()
                .unwrap()
                .into_owned(),
            utxo_id
        );
    }

    #[test]
    fn latest_utxo_put() {
        let contract_id: ContractId = ContractId::from([1u8; 32]);
        let utxo_id: UtxoId = UtxoId::new(TxId::new([2u8; 32]), 4);

        let database = &mut Database::default();
        database
            .storage::<ContractsLatestUtxo>()
            .insert(&contract_id, &utxo_id)
            .unwrap();

        let returned: UtxoId = *database
            .storage::<ContractsLatestUtxo>()
            .get(&contract_id)
            .unwrap()
            .unwrap();
        assert_eq!(returned, utxo_id);
    }

    #[test]
    fn latest_utxo_remove() {
        let contract_id: ContractId = ContractId::from([1u8; 32]);
        let utxo_id: UtxoId = UtxoId::new(TxId::new([2u8; 32]), 4);

        let database = &mut Database::default();
        database
            .storage::<ContractsLatestUtxo>()
            .insert(&contract_id, &utxo_id)
            .unwrap();

        database
            .storage::<ContractsLatestUtxo>()
            .remove(&contract_id)
            .unwrap();

        assert!(!database
            .storage::<ContractsLatestUtxo>()
            .contains_key(&contract_id)
            .unwrap());
    }

    #[test]
    fn latest_utxo_exists() {
        let contract_id: ContractId = ContractId::from([1u8; 32]);
        let utxo_id: UtxoId = UtxoId::new(TxId::new([2u8; 32]), 4);

        let database = &mut Database::default();
        database
            .storage::<ContractsLatestUtxo>()
            .insert(&contract_id, &utxo_id)
            .unwrap();

        assert!(database
            .storage::<ContractsLatestUtxo>()
            .contains_key(&contract_id)
            .unwrap());
    }
}
