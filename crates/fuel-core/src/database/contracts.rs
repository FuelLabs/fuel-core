use crate::database::Database;
use fuel_core_chain_config::{
    ContractBalanceConfig,
    ContractConfig,
    ContractStateConfig,
};
use fuel_core_storage::{
    iter::IterDirection,
    not_found,
    tables::{
        ContractsAssets,
        ContractsInfo,
        ContractsLatestUtxo,
        ContractsRawCode,
        ContractsState,
    },
    ContractsAssetKey,
    Result as StorageResult,
    StorageAsRef,
};
use fuel_core_types::{
    fuel_tx::Bytes32,
    fuel_types::{
        AssetId,
        ContractId,
        Word,
    },
};

impl Database {
    pub fn iter_contract_state_configs(
        &self,
    ) -> impl Iterator<Item = StorageResult<ContractStateConfig>> + '_ {
        self.iter_all::<ContractsState>(None).map(|res| {
            let (key, value) = res?;
            let contract_id = *key.contract_id();
            let key = *key.state_key();

            Ok(ContractStateConfig {
                contract_id,
                key,
                value: value.0,
            })
        })
    }

    pub fn iter_contract_balance_configs(
        &self,
    ) -> impl Iterator<Item = StorageResult<ContractBalanceConfig>> + '_ {
        self.iter_all::<ContractsAssets>(None).map(|res| {
            let res = res?;

            let contract_id = *res.0.contract_id();
            let asset_id = *res.0.asset_id();

            Ok(ContractBalanceConfig {
                contract_id,
                asset_id,
                amount: res.1,
            })
        })
    }

    pub fn get_contract_config(
        &self,
        contract_id: ContractId,
    ) -> StorageResult<ContractConfig> {
        let code = self
            .storage::<ContractsRawCode>()
            .get(&contract_id)?
            .map(|v| {
                let code: Vec<u8> = v.into_owned().into();
                code
            })
            .ok_or_else(|| not_found!("ContractsRawCode"))?;

        let salt = *self
            .storage::<ContractsInfo>()
            .get(&contract_id)
            .unwrap()
            .expect("Contract does not exist")
            .salt();

        let latest_utxo = self
            .storage::<ContractsLatestUtxo>()
            .get(&contract_id)
            .unwrap()
            .expect("contract does not exist")
            .into_owned();
        let utxo_id = latest_utxo.utxo_id();
        let tx_pointer = latest_utxo.tx_pointer();

        Ok(ContractConfig {
            contract_id,
            code,
            salt,
            tx_id: Some(*utxo_id.tx_id()),
            output_index: Some(utxo_id.output_index()),
            tx_pointer_block_height: Some(tx_pointer.block_height()),
            tx_pointer_tx_idx: Some(tx_pointer.tx_index()),
        })
    }

    pub fn contract_balances(
        &self,
        contract: ContractId,
        start_asset: Option<AssetId>,
        direction: Option<IterDirection>,
    ) -> impl Iterator<Item = StorageResult<(AssetId, Word)>> + '_ {
        let start_asset =
            start_asset.map(|asset| ContractsAssetKey::new(&contract, &asset));
        self.iter_all_filtered::<ContractsAssets, _>(
            Some(contract),
            start_asset.as_ref(),
            direction,
        )
        .map(|res| res.map(|(key, balance)| (*key.asset_id(), balance)))
    }

    pub fn contract_states(
        &self,
        contract_id: ContractId,
    ) -> impl Iterator<Item = StorageResult<(Bytes32, Vec<u8>)>> + '_ {
        self.iter_all_by_prefix::<ContractsState, _>(Some(contract_id))
            .map(|res| -> StorageResult<(Bytes32, Vec<u8>)> {
                let (key, value) = res?;

                Ok((*key.state_key(), value.0))
            })
    }

    pub fn iter_contract_configs(
        &self,
    ) -> impl Iterator<Item = StorageResult<ContractConfig>> + '_ {
        self.iter_all::<ContractsRawCode>(None).map(
            |raw_contract_id| -> StorageResult<ContractConfig> {
                let contract_id = raw_contract_id?.0;
                self.get_contract_config(contract_id)
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::database_description::on_chain::OnChain;
    use fuel_core_storage::StorageAsMut;
    use fuel_core_types::fuel_tx::Contract;
    use rand::{
        RngCore,
        SeedableRng,
    };

    #[test]
    fn raw_code_put_huge_contract() {
        let rng = &mut rand::rngs::StdRng::seed_from_u64(2322u64);
        let contract_id: ContractId = ContractId::from([1u8; 32]);
        let mut bytes = vec![0; 16 * 1024 * 1024];
        rng.fill_bytes(bytes.as_mut());
        let contract: Contract = Contract::from(bytes);

        let database = &mut Database::<OnChain>::default();
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
}
