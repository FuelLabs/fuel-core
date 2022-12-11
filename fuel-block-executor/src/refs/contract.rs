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
    executor::Error,
};
use std::borrow::Cow;

/// The wrapper around `contract_id` to simplify work with `Contract` in the database.
pub struct ContractRef<Database> {
    database: Database,
    contract_id: ContractId,
}

impl<Database> ContractRef<Database> {
    pub fn new(database: Database, contract_id: ContractId) -> Self {
        Self {
            database,
            contract_id,
        }
    }

    pub fn contract_id(&self) -> &ContractId {
        &self.contract_id
    }

    pub fn database(&self) -> &Database {
        &self.database
    }

    pub fn database_mut(&mut self) -> &mut Database {
        &mut self.database
    }
}

impl<Database> ContractRef<Database>
where
    Database: StorageInspect<ContractsLatestUtxo>,
    Error: From<Database::Error>,
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

impl<Database> ContractRef<Database>
where
    Database: StorageInspect<ContractsLatestUtxo>,
    Error: From<Database::Error>,
{
    pub fn validated_utxo(
        &self,
        utxo_validation: bool,
    ) -> Result<<ContractsLatestUtxo as Mappable>::GetValue, Error> {
        let maybe_utxo_id = self.utxo()?.map(|utxo| utxo.into_owned());
        let expected_utxo_id = if utxo_validation {
            maybe_utxo_id.ok_or(Error::ContractUtxoMissing(self.contract_id))?
        } else {
            maybe_utxo_id.unwrap_or_default()
        };
        Result::<_, Error>::Ok(expected_utxo_id)
    }
}

impl<Database> ContractRef<Database>
where
    for<'b> Database: MerkleRootStorage<ContractId, ContractsAssets<'b>>,
{
    pub fn balance_root(
        &mut self,
    ) -> Result<Bytes32, <Database as StorageInspect<ContractsAssets<'_>>>::Error> {
        self.database.root(&self.contract_id).map(Into::into)
    }
}

impl<Database> ContractRef<Database>
where
    for<'b> Database: MerkleRootStorage<ContractId, ContractsState<'b>>,
{
    pub fn state_root(
        &mut self,
    ) -> Result<Bytes32, <Database as StorageInspect<ContractsState<'_>>>::Error> {
        self.database.root(&self.contract_id).map(Into::into)
    }
}
