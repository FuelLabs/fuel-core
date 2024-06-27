use crate::database::OnChainIterableKeyValueView;
use fuel_core_chain_config::TableEntry;
use fuel_core_storage::{
    iter::{
        IterDirection,
        IteratorOverTable,
    },
    not_found,
    tables::{
        ContractsAssets,
        ContractsLatestUtxo,
        ContractsRawCode,
        ContractsState,
    },
    ContractsAssetKey,
    Result as StorageResult,
    StorageAsRef,
};
use fuel_core_types::fuel_types::{
    AssetId,
    ContractId,
};
use itertools::Itertools;

impl OnChainIterableKeyValueView {
    pub fn iter_contract_state(
        &self,
    ) -> impl Iterator<Item = StorageResult<TableEntry<ContractsState>>> + '_ {
        self.iter_all::<ContractsState>(None)
            .map_ok(|(key, value)| TableEntry { key, value })
    }

    pub fn iter_contract_balance(
        &self,
    ) -> impl Iterator<Item = StorageResult<TableEntry<ContractsAssets>>> + '_ {
        self.iter_all::<ContractsAssets>(None)
            .map_ok(|(key, value)| TableEntry { key, value })
    }

    pub fn iter_contracts_code(
        &self,
    ) -> impl Iterator<Item = StorageResult<TableEntry<ContractsRawCode>>> + '_ {
        self.iter_all::<ContractsRawCode>(None)
            .map_ok(|(key, value)| TableEntry { key, value })
    }

    pub fn iter_contracts_latest_utxo(
        &self,
    ) -> impl Iterator<Item = StorageResult<TableEntry<ContractsLatestUtxo>>> + '_ {
        self.iter_all::<ContractsLatestUtxo>(None)
            .map_ok(|(key, value)| TableEntry { key, value })
    }

    pub fn contract_code(
        &self,
        contract_id: ContractId,
    ) -> StorageResult<TableEntry<ContractsRawCode>> {
        self.storage::<ContractsRawCode>()
            .get(&contract_id)?
            .map(|value| TableEntry {
                key: contract_id,
                value: value.into_owned(),
            })
            .ok_or_else(|| not_found!("ContractsRawCode"))
    }

    pub fn contract_latest_utxo(
        &self,
        contract_id: ContractId,
    ) -> StorageResult<TableEntry<ContractsLatestUtxo>> {
        self.storage::<ContractsLatestUtxo>()
            .get(&contract_id)?
            .map(|value| TableEntry {
                key: contract_id,
                value: value.into_owned(),
            })
            .ok_or_else(|| not_found!("ContractsLatestUtxo"))
    }

    pub fn filter_contract_balances(
        &self,
        contract: ContractId,
        start_asset: Option<AssetId>,
        direction: Option<IterDirection>,
    ) -> impl Iterator<Item = StorageResult<TableEntry<ContractsAssets>>> + '_ {
        let start_asset =
            start_asset.map(|asset| ContractsAssetKey::new(&contract, &asset));
        self.iter_all_filtered::<ContractsAssets, _>(
            Some(contract),
            start_asset.as_ref(),
            direction,
        )
        .map_ok(|(key, value)| TableEntry { key, value })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::{
        database_description::on_chain::OnChain,
        Database,
    };
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
