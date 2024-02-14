use crate::database::Database;
use fuel_core_chain_config::ContractConfig;
use fuel_core_storage::{
    iter::IterDirection,
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
    entities::contract::ContractUtxoInfo,
    fuel_types::{
        AssetId,
        Bytes32,
        ContractId,
        Word,
    },
};

impl Database {
    pub fn get_contract_config_by_id(
        &self,
        contract_id: ContractId,
    ) -> StorageResult<ContractConfig> {
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

        let ContractUtxoInfo {
            utxo_id,
            tx_pointer,
        } = self
            .storage::<ContractsLatestUtxo>()
            .get(&contract_id)
            .unwrap()
            .expect("contract does not exist")
            .into_owned();

        let state = Some(
            self.iter_all_by_prefix::<ContractsState, _>(Some(contract_id.as_ref()))
                .map(|res| -> StorageResult<(Bytes32, Bytes32)> {
                    let (key, value) = res?;

                    Ok((*key.state_key(), value))
                })
                .filter(|val| val.is_ok())
                .collect::<StorageResult<Vec<_>>>()?,
        );

        let balances = Some(
            self.iter_all_by_prefix::<ContractsAssets, _>(Some(contract_id.as_ref()))
                .map(|res| {
                    let (key, value) = res?;

                    Ok((*key.asset_id(), value))
                })
                .filter(|val| val.is_ok())
                .collect::<StorageResult<Vec<_>>>()?,
        );

        Ok(ContractConfig {
            contract_id,
            code,
            salt,
            state,
            balances,
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

    pub fn get_contract_config(&self) -> StorageResult<Option<Vec<ContractConfig>>> {
        let configs = self
            .iter_all::<ContractsRawCode>(None)
            .map(|raw_contract_id| -> StorageResult<ContractConfig> {
                let contract_id = raw_contract_id?.0;
                self.get_contract_config_by_id(contract_id)
            })
            .collect::<StorageResult<Vec<ContractConfig>>>()?;

        Ok(Some(configs))
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
