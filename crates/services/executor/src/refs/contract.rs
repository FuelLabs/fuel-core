use core::fmt;
use fuel_core_storage::{
    not_found,
    tables::{
        ContractsAssets,
        ContractsLatestUtxo,
        ContractsState,
    },
    Error as StorageError,
    Mappable,
    MerkleRoot,
    MerkleRootStorage,
    StorageAsRef,
    StorageInspect,
};
use fuel_core_types::{
    fuel_crypto::Hasher,
    fuel_types::{
        Bytes32,
        ContractId,
    },
    services::executor::{
        Error as ExecutorError,
        Result as ExecutorResult,
    },
};

#[cfg(feature = "std")]
use std::borrow::Cow;

#[cfg(not(feature = "std"))]
use alloc::borrow::Cow;

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
    ExecutorError: From<Database::Error>,
{
    pub fn utxo(
        &self,
    ) -> Result<
        Option<Cow<'_, <ContractsLatestUtxo as Mappable>::OwnedValue>>,
        Database::Error,
    > {
        self.database.storage().get(&self.contract_id)
    }
}

impl<Database> ContractRef<Database>
where
    Database: StorageInspect<ContractsLatestUtxo>,
    ExecutorError: From<Database::Error>,
{
    pub fn validated_utxo(
        &self,
        utxo_validation: bool,
    ) -> ExecutorResult<<ContractsLatestUtxo as Mappable>::OwnedValue> {
        let maybe_utxo_id = self.utxo()?.map(|utxo| utxo.into_owned());
        let expected_utxo_id = if utxo_validation {
            maybe_utxo_id.ok_or(ExecutorError::ContractUtxoMissing(self.contract_id))?
        } else {
            maybe_utxo_id.unwrap_or_default()
        };
        Ok(expected_utxo_id)
    }
}

impl<Database> ContractRef<Database>
where
    Database: MerkleRootStorage<ContractId, ContractsAssets>,
{
    pub fn balance_root(
        &self,
    ) -> Result<Bytes32, <Database as StorageInspect<ContractsAssets>>::Error> {
        self.database.root(&self.contract_id).map(Into::into)
    }
}

impl<Database> ContractRef<Database>
where
    Database: MerkleRootStorage<ContractId, ContractsState>,
{
    pub fn state_root(
        &self,
    ) -> Result<Bytes32, <Database as StorageInspect<ContractsState>>::Error> {
        self.database.root(&self.contract_id).map(Into::into)
    }
}

pub trait ContractStorageTrait:
    StorageInspect<ContractsLatestUtxo, Error = Self::InnerError>
    + MerkleRootStorage<ContractId, ContractsState, Error = Self::InnerError>
    + MerkleRootStorage<ContractId, ContractsAssets, Error = Self::InnerError>
{
    type InnerError: fmt::Debug + fmt::Display + Send + Sync + 'static;
}

impl<D> ContractStorageTrait for D
where
    D: StorageInspect<ContractsLatestUtxo, Error = StorageError>
        + MerkleRootStorage<ContractId, ContractsState, Error = StorageError>
        + MerkleRootStorage<ContractId, ContractsAssets, Error = StorageError>,
{
    type InnerError = StorageError;
}

impl<'a, Database> ContractRef<&'a Database>
where
    Database: ContractStorageTrait,
    anyhow::Error: From<Database::InnerError>,
{
    /// Returns the state root of the whole contract.
    pub fn root(&self) -> anyhow::Result<MerkleRoot> {
        let contract_id = *self.contract_id();
        let utxo = *self
            .database()
            .storage::<ContractsLatestUtxo>()
            .get(&contract_id)?
            .ok_or(not_found!(ContractsLatestUtxo))?
            .into_owned()
            .utxo_id();

        let state_root = self
            .database
            .storage::<ContractsState>()
            .root(&contract_id)?;

        let balance_root = self
            .database
            .storage::<ContractsAssets>()
            .root(&contract_id)?;

        let contract_hash = *Hasher::default()
            // `ContractId` already is based on contract's code and salt so we don't need it.
            .chain(contract_id.as_ref())
            .chain(utxo.tx_id().as_ref())
            .chain(utxo.output_index().to_be_bytes())
            .chain(state_root.as_slice())
            .chain(balance_root.as_slice())
            .finalize();

        Ok(contract_hash)
    }
}
