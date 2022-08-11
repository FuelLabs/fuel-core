use crate::database::InterpreterStorage;
use crate::{
    chain_config::ContractConfig,
    database::{
        columns::{BALANCES, CONTRACTS, CONTRACTS_STATE, CONTRACT_UTXO_ID},
        Database,
    },
    state::{Error, IterDirection, MultiKey},
};
use fuel_core_interfaces::common::{
    fuel_tx::UtxoId,
    fuel_types::{Bytes32, Word},
    fuel_vm::prelude::{AssetId, Contract, ContractId, Storage},
};
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

impl Storage<ContractId, UtxoId> for Database {
    type Error = Error;

    fn insert(&mut self, key: &ContractId, value: &UtxoId) -> Result<Option<UtxoId>, Self::Error> {
        Database::insert(self, key.as_ref(), CONTRACT_UTXO_ID, *value)
    }

    fn remove(&mut self, key: &ContractId) -> Result<Option<UtxoId>, Self::Error> {
        Database::remove(self, key.as_ref(), CONTRACT_UTXO_ID)
    }

    fn get<'a>(&'a self, key: &ContractId) -> Result<Option<Cow<'a, UtxoId>>, Self::Error> {
        self.get(key.as_ref(), CONTRACT_UTXO_ID)
    }

    fn contains_key(&self, key: &ContractId) -> Result<bool, Self::Error> {
        self.exists(key.as_ref(), CONTRACT_UTXO_ID)
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
            BALANCES,
            Some(contract.as_ref().to_vec()),
            start_asset.map(|asset_id| MultiKey::new((&contract, &asset_id)).as_ref().to_vec()),
            direction,
        )
        .map(|res| res.map(|(key, balance)| (AssetId::new(key[32..].try_into().unwrap()), balance)))
    }

    pub fn get_contract_config(&self) -> Result<Option<Vec<ContractConfig>>, anyhow::Error> {
        let configs = self
            .iter_all::<Vec<u8>, Word>(CONTRACTS, None, None, None)
            .map(|raw_contract_id| -> Result<ContractConfig, anyhow::Error> {
                let contract_id = ContractId::new(raw_contract_id.unwrap().0[..32].try_into()?);

                let code: Vec<u8> = Storage::<ContractId, Contract>::get(self, &contract_id)?
                    .unwrap()
                    .into_owned()
                    .into();

                let salt = InterpreterStorage::storage_contract_root(self, &contract_id)?
                    .unwrap()
                    .into_owned()
                    .0;

                let state = Some(
                    self.iter_all::<Vec<u8>, Bytes32>(
                        CONTRACTS_STATE,
                        Some(contract_id.as_ref().to_vec()),
                        None,
                        None,
                    )
                    .map(|res| -> Result<(Bytes32, Bytes32), anyhow::Error> {
                        let safe_res = res?;

                        // We don't need to store ContractId which is the first 32 bytes of this
                        // key, as this Vec is already attached to that ContractId
                        let state_key = Bytes32::new(safe_res.0[32..].try_into()?);

                        Ok((state_key, safe_res.1))
                    })
                    .filter(|val| val.is_ok())
                    .collect::<Result<Vec<(Bytes32, Bytes32)>, anyhow::Error>>()?,
                );

                let balances = Some(
                    self.iter_all::<Vec<u8>, u64>(
                        BALANCES,
                        Some(contract_id.as_ref().to_vec()),
                        None,
                        None,
                    )
                    .map(|res| -> Result<(AssetId, u64), anyhow::Error> {
                        let safe_res = res?;

                        let asset_id = AssetId::new(safe_res.0[32..].try_into()?);

                        Ok((asset_id, safe_res.1))
                    })
                    .filter(|val| val.is_ok())
                    .collect::<Result<Vec<(AssetId, u64)>, anyhow::Error>>()?,
                );

                Ok(ContractConfig {
                    code,
                    salt,
                    state,
                    balances,
                })
            })
            .collect::<Result<Vec<ContractConfig>, anyhow::Error>>()?;

        Ok(Some(configs))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fuel_core_interfaces::common::fuel_tx::TxId;

    #[test]
    fn contract_get() {
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
    fn contract_put() {
        let contract_id: ContractId = ContractId::from([1u8; 32]);
        let contract: Contract = Contract::from(vec![32u8]);

        let mut database = Database::default();
        Storage::<ContractId, Contract>::insert(&mut database, &contract_id, &contract).unwrap();

        let returned: Contract = database
            .get(contract_id.as_ref(), CONTRACTS)
            .unwrap()
            .unwrap();
        assert_eq!(returned, contract);
    }

    #[test]
    fn contract_remove() {
        let contract_id: ContractId = ContractId::from([1u8; 32]);
        let contract: Contract = Contract::from(vec![32u8]);

        let mut database = Database::default();
        database
            .insert(contract_id.as_ref().to_vec(), CONTRACTS, contract)
            .unwrap();

        Storage::<ContractId, Contract>::remove(&mut database, &contract_id).unwrap();

        assert!(!database.exists(contract_id.as_ref(), CONTRACTS).unwrap());
    }

    #[test]
    fn contract_exists() {
        let contract_id: ContractId = ContractId::from([1u8; 32]);
        let contract: Contract = Contract::from(vec![32u8]);

        let database = Database::default();
        database
            .insert(contract_id.as_ref().to_vec(), CONTRACTS, contract)
            .unwrap();

        assert!(Storage::<ContractId, Contract>::contains_key(&database, &contract_id).unwrap());
    }

    #[test]
    fn contract_utxo_id_get() {
        let contract_id: ContractId = ContractId::from([1u8; 32]);
        let utxo_id: UtxoId = UtxoId::new(TxId::new([2u8; 32]), 4);

        let database = Database::default();

        database
            .insert(contract_id.as_ref().to_vec(), CONTRACT_UTXO_ID, utxo_id)
            .unwrap();

        assert_eq!(
            Storage::<ContractId, UtxoId>::get(&database, &contract_id)
                .unwrap()
                .unwrap()
                .into_owned(),
            utxo_id
        );
    }

    #[test]
    fn contract_utxo_id_put() {
        let contract_id: ContractId = ContractId::from([1u8; 32]);
        let utxo_id: UtxoId = UtxoId::new(TxId::new([2u8; 32]), 4);

        let mut database = Database::default();
        Storage::<ContractId, UtxoId>::insert(&mut database, &contract_id, &utxo_id).unwrap();

        let returned: UtxoId = database
            .get(contract_id.as_ref(), CONTRACT_UTXO_ID)
            .unwrap()
            .unwrap();
        assert_eq!(returned, utxo_id);
    }

    #[test]
    fn contract_utxo_id_remove() {
        let contract_id: ContractId = ContractId::from([1u8; 32]);
        let utxo_id: UtxoId = UtxoId::new(TxId::new([2u8; 32]), 4);

        let mut database = Database::default();
        database
            .insert(contract_id.as_ref().to_vec(), CONTRACT_UTXO_ID, utxo_id)
            .unwrap();

        Storage::<ContractId, UtxoId>::remove(&mut database, &contract_id).unwrap();

        assert!(!database
            .exists(contract_id.as_ref(), CONTRACT_UTXO_ID)
            .unwrap());
    }

    #[test]
    fn contract_utxo_id_exists() {
        let contract_id: ContractId = ContractId::from([1u8; 32]);
        let utxo_id: UtxoId = UtxoId::new(TxId::new([2u8; 32]), 4);

        let database = Database::default();
        database
            .insert(contract_id.as_ref().to_vec(), CONTRACT_UTXO_ID, utxo_id)
            .unwrap();

        assert!(Storage::<ContractId, UtxoId>::contains_key(&database, &contract_id).unwrap());
    }
}
