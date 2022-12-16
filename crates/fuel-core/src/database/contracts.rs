use crate::{
    database::{
        Column,
        Database,
        Error as DatabaseError,
        Result as DatabaseResult,
    },
    state::{
        IterDirection,
        MultiKey,
    },
};
use fuel_core_chain_config::ContractConfig;
use fuel_core_storage::{
    tables::{
        ContractsInfo,
        ContractsLatestUtxo,
        ContractsRawCode,
    },
    Error as StorageError,
    Result as StorageResult,
    StorageAsRef,
    StorageInspect,
    StorageMutate,
};
use fuel_core_types::{
    fuel_tx::{
        Contract,
        UtxoId,
    },
    fuel_types::{
        AssetId,
        Bytes32,
        ContractId,
        Word,
    },
};

use std::borrow::Cow;

impl StorageInspect<ContractsRawCode> for Database {
    type Error = StorageError;

    fn get(&self, key: &ContractId) -> Result<Option<Cow<Contract>>, Self::Error> {
        self.get(key.as_ref(), Column::ContractsRawCode)
            .map_err(Into::into)
    }

    fn contains_key(&self, key: &ContractId) -> Result<bool, Self::Error> {
        self.exists(key.as_ref(), Column::ContractsRawCode)
            .map_err(Into::into)
    }
}

impl StorageMutate<ContractsRawCode> for Database {
    fn insert(
        &mut self,
        key: &ContractId,
        value: &[u8],
    ) -> Result<Option<Contract>, Self::Error> {
        Database::insert(self, key.as_ref(), Column::ContractsRawCode, value)
            .map_err(Into::into)
    }

    fn remove(&mut self, key: &ContractId) -> Result<Option<Contract>, Self::Error> {
        Database::remove(self, key.as_ref(), Column::ContractsRawCode).map_err(Into::into)
    }
}

impl StorageInspect<ContractsLatestUtxo> for Database {
    type Error = StorageError;

    fn get(&self, key: &ContractId) -> Result<Option<Cow<UtxoId>>, Self::Error> {
        self.get(key.as_ref(), Column::ContractsLatestUtxo)
            .map_err(Into::into)
    }

    fn contains_key(&self, key: &ContractId) -> Result<bool, Self::Error> {
        self.exists(key.as_ref(), Column::ContractsLatestUtxo)
            .map_err(Into::into)
    }
}

impl StorageMutate<ContractsLatestUtxo> for Database {
    fn insert(
        &mut self,
        key: &ContractId,
        value: &UtxoId,
    ) -> Result<Option<UtxoId>, Self::Error> {
        Database::insert(self, key.as_ref(), Column::ContractsLatestUtxo, *value)
            .map_err(Into::into)
    }

    fn remove(&mut self, key: &ContractId) -> Result<Option<UtxoId>, Self::Error> {
        Database::remove(self, key.as_ref(), Column::ContractsLatestUtxo)
            .map_err(Into::into)
    }
}

impl Database {
    pub fn contract_balances(
        &self,
        contract: ContractId,
        start_asset: Option<AssetId>,
        direction: Option<IterDirection>,
    ) -> impl Iterator<Item = DatabaseResult<(AssetId, Word)>> + '_ {
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

    pub fn get_contract_config(&self) -> StorageResult<Option<Vec<ContractConfig>>> {
        let configs = self
            .iter_all::<Vec<u8>, Word>(Column::ContractsRawCode, None, None, None)
            .map(|raw_contract_id| -> StorageResult<ContractConfig> {
                let contract_id = ContractId::new(
                    raw_contract_id.unwrap().0[..32]
                        .try_into()
                        .map_err(DatabaseError::from)?,
                );

                let code: Vec<u8> = self
                    .storage::<ContractsRawCode>()
                    .get(&contract_id)?
                    .unwrap()
                    .into_owned()
                    .into();

                let (salt, _) = self
                    .storage::<ContractsInfo>()
                    .get(&contract_id)
                    .unwrap()
                    .expect("Contract does not exist")
                    .into_owned();

                let state = Some(
                    self.iter_all::<Vec<u8>, Bytes32>(
                        Column::ContractsState,
                        Some(contract_id.as_ref().to_vec()),
                        None,
                        None,
                    )
                    .map(|res| -> DatabaseResult<(Bytes32, Bytes32)> {
                        let safe_res = res?;

                        // We don't need to store ContractId which is the first 32 bytes of this
                        // key, as this Vec is already attached to that ContractId
                        let state_key = Bytes32::new(safe_res.0[32..].try_into()?);

                        Ok((state_key, safe_res.1))
                    })
                    .filter(|val| val.is_ok())
                    .collect::<DatabaseResult<Vec<(Bytes32, Bytes32)>>>()?,
                );

                let balances = Some(
                    self.iter_all::<Vec<u8>, u64>(
                        Column::ContractsAssets,
                        Some(contract_id.as_ref().to_vec()),
                        None,
                        None,
                    )
                    .map(|res| {
                        let safe_res = res?;

                        let asset_id = AssetId::new(
                            safe_res.0[32..].try_into().map_err(DatabaseError::from)?,
                        );

                        Ok((asset_id, safe_res.1))
                    })
                    .filter(|val| val.is_ok())
                    .collect::<StorageResult<Vec<(AssetId, u64)>>>()?,
                );

                Ok(ContractConfig {
                    code,
                    salt,
                    state,
                    balances,
                })
            })
            .collect::<StorageResult<Vec<ContractConfig>>>()?;

        Ok(Some(configs))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fuel_core_storage::StorageAsMut;
    use fuel_core_types::fuel_tx::TxId;

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
