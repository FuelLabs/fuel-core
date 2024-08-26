use core::fmt;
use fuel_core_storage::{
    not_found,
    tables::ContractsLatestUtxo,
    Error as StorageError,
    Mappable,
    MerkleRoot,
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

#[cfg(feature = "smt")]
pub use smt::*;

#[cfg(not(feature = "smt"))]
pub use not_smt::*;

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

#[cfg(feature = "smt")]
mod smt {
    use super::*;
    use fuel_core_storage::{
        tables::{
            ContractsAssets,
            ContractsState,
        },
        MerkleRootStorage,
    };

    impl<Database> ContractRef<Database>
    where
        Database: ContractStorageTrait,
    {
        pub fn balance_root(
            &self,
        ) -> Result<Bytes32, <Database as StorageInspect<ContractsAssets>>::Error>
        {
            <Database as MerkleRootStorage<ContractId, ContractsAssets>>::root(
                &self.database,
                &self.contract_id,
            )
            .map(Into::into)
        }

        pub fn state_root(
            &self,
        ) -> Result<Bytes32, <Database as StorageInspect<ContractsState>>::Error>
        {
            <Database as MerkleRootStorage<ContractId, ContractsState>>::root(
                &self.database,
                &self.contract_id,
            )
            .map(Into::into)
        }
    }

    pub trait ContractStorageTrait:
        StorageInspect<ContractsLatestUtxo, Error = Self::InnerError>
        + MerkleRootStorage<ContractId, ContractsState, Error = Self::InnerError>
        + MerkleRootStorage<ContractId, ContractsAssets, Error = Self::InnerError>
    {
        type InnerError: fmt::Debug + fmt::Display + Send + Sync + 'static;
    }

    impl<D, E> ContractStorageTrait for D
    where
        D: StorageInspect<ContractsLatestUtxo, Error = E>
            + MerkleRootStorage<ContractId, ContractsState, Error = E>
            + MerkleRootStorage<ContractId, ContractsAssets, Error = E>,
        E: fmt::Debug + fmt::Display + Send + Sync + 'static,
    {
        type InnerError = E;
    }
}

#[cfg(not(feature = "smt"))]
mod not_smt {
    use super::*;
    use fuel_core_storage::Error as StorageError;

    impl<Database> ContractRef<Database> {
        pub fn balance_root(&self) -> Result<Bytes32, StorageError> {
            Ok(Bytes32::zeroed())
        }
    }

    impl<Database> ContractRef<Database> {
        pub fn state_root(&self) -> Result<Bytes32, StorageError> {
            Ok(Bytes32::zeroed())
        }
    }

    pub trait ContractStorageTrait:
        StorageInspect<ContractsLatestUtxo, Error = Self::InnerError>
    {
        type InnerError: fmt::Debug + fmt::Display + Send + Sync + 'static;
    }

    impl<D, E> ContractStorageTrait for D
    where
        D: StorageInspect<ContractsLatestUtxo, Error = E>,
        E: fmt::Debug + fmt::Display + Send + Sync + 'static,
    {
        type InnerError = E;
    }
}

impl<'a, Database> ContractRef<&'a Database>
where
    &'a Database: ContractStorageTrait<InnerError = StorageError>,
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

        let state_root = self.state_root()?;

        let balance_root = self.balance_root()?;

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
