use crate::fuel_core_graphql_api::ports::{
    OffChainDatabase,
    OnChainDatabase,
};
use fuel_core_storage::{
    iter::{
        BoxedIter,
        IterDirection,
    },
    not_found,
    tables::{
        ContractsAssets,
        ContractsRawCode,
    },
    Result as StorageResult,
    StorageAsRef,
};
use fuel_core_types::{
    fuel_types::{
        AssetId,
        ContractId,
    },
    fuel_vm::Salt,
    services::graphql_api::ContractBalance,
};

pub trait ContractQueryData: Send + Sync {
    fn contract_exists(&self, id: ContractId) -> StorageResult<bool>;

    fn contract_bytecode(&self, id: ContractId) -> StorageResult<Vec<u8>>;

    fn contract_salt(&self, id: ContractId) -> StorageResult<Salt>;

    fn contract_balance(
        &self,
        contract_id: ContractId,
        asset_id: AssetId,
    ) -> StorageResult<ContractBalance>;

    fn contract_balances(
        &self,
        contract_id: ContractId,
        start_asset: Option<AssetId>,
        direction: IterDirection,
    ) -> BoxedIter<StorageResult<ContractBalance>>;
}

impl<D: OnChainDatabase + OffChainDatabase + ?Sized> ContractQueryData for D {
    fn contract_exists(&self, id: ContractId) -> StorageResult<bool> {
        self.storage::<ContractsRawCode>().contains_key(&id)
    }

    fn contract_bytecode(&self, id: ContractId) -> StorageResult<Vec<u8>> {
        let contract = self
            .storage::<ContractsRawCode>()
            .get(&id)?
            .ok_or(not_found!(ContractsRawCode))?
            .into_owned();

        Ok(contract.into())
    }

    fn contract_salt(&self, id: ContractId) -> StorageResult<Salt> {
        self.contract_salt(&id)
    }

    fn contract_balance(
        &self,
        contract_id: ContractId,
        asset_id: AssetId,
    ) -> StorageResult<ContractBalance> {
        let amount = self
            .storage::<ContractsAssets>()
            .get(&(&contract_id, &asset_id).into())?
            .ok_or(not_found!(ContractsAssets))?
            .into_owned();

        Ok(ContractBalance {
            owner: contract_id,
            amount,
            asset_id,
        })
    }

    fn contract_balances(
        &self,
        contract_id: ContractId,
        start_asset: Option<AssetId>,
        direction: IterDirection,
    ) -> BoxedIter<StorageResult<ContractBalance>> {
        self.contract_balances(contract_id, start_asset, direction)
    }
}
