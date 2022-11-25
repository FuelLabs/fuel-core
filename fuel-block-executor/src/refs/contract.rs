use fuel_core_interfaces::{
    common::{
        fuel_storage::{
            Mappable,
            MerkleRootStorage,
            StorageAsRef,
            StorageInspect,
        },
        fuel_tx::{
            Bytes32,
            ContractId,
        },
    },
    db::{
        ContractsAssets,
        ContractsLatestUtxo,
        ContractsState,
    },
};
use std::borrow::Cow;

/// The wrapper around `contract_id` to simplify work with `Contract` in the database.
pub struct ContractRef<'a, Database> {
    database: Database,
    contract_id: &'a ContractId,
}

impl<'a, Database> ContractRef<'a, Database> {
    pub fn new(database: Database, contract_id: &'a ContractId) -> Self {
        Self {
            database,
            contract_id,
        }
    }

    pub fn contract_id(&self) -> &ContractId {
        self.contract_id
    }
}

impl<'a, Database> ContractRef<'a, Database>
where
    Database: StorageInspect<ContractsLatestUtxo>,
{
    pub fn utxo(
        &self,
    ) -> Result<
        Option<Cow<'_, <ContractsLatestUtxo as Mappable>::GetValue>>,
        Database::Error,
    > {
        self.database.storage().get(&self.contract_id)
    }
}

impl<'a, Database> ContractRef<'a, Database>
where
    for<'b> Database: MerkleRootStorage<ContractId, ContractsAssets<'b>>,
{
    pub fn balance_root(
        &mut self,
    ) -> Result<Bytes32, <Database as StorageInspect<ContractsAssets<'_>>>::Error> {
        self.database.root(&self.contract_id).map(Into::into)
    }
}

impl<'a, Database> ContractRef<'a, Database>
where
    for<'b> Database: MerkleRootStorage<ContractId, ContractsState<'b>>,
{
    pub fn state_root(
        &mut self,
    ) -> Result<Bytes32, <Database as StorageInspect<ContractsState<'_>>>::Error> {
        self.database.root(&self.contract_id).map(Into::into)
    }
}
