//! This module contains local mirrors for the tables from fuel-core-storage
//! used for the state root service.
//!
//! The following tables are mirrored:
//! - `ContractsRawCode` table.
//! - `ContractsLatestUtxo` table.
//! - `Coins` table.
//! - `Messages` table.
//! - `ProcessedTransactions` table.
//! - `ConsensusParametersVersions` table.
//! - `StateTransitionBytecodeVersions` table.
//! - `UploadedBytecodes` table.
//! - `Blobs` table.

use fuel_core_storage::Mappable;

/// Local mirror of the [`fuel_core_storage::tables::ContractsRawCode`] table.
pub type ContractsRawCode = LocalMirror<fuel_core_storage::tables::ContractsRawCode>;

/// Local mirror of the [`fuel_core_storage::tables::ContractsLatestUtxo`] table.
pub type ContractsLatestUtxo =
    LocalMirror<fuel_core_storage::tables::ContractsLatestUtxo>;

/// Local mirror of the [`fuel_core_storage::tables::Coins`] table.
pub type Coins = LocalMirror<fuel_core_storage::tables::Coins>;

/// Local mirror of the [`fuel_core_storage::tables::Messages`] table.
pub type Messages = LocalMirror<fuel_core_storage::tables::Messages>;

/// Local mirror of the [`fuel_core_storage::tables::ProcessedTransactions`] table.
pub type ProcessedTransactions =
    LocalMirror<fuel_core_storage::tables::ProcessedTransactions>;

/// Local mirror of the [`fuel_core_storage::tables::ConsensusParametersVersions`] table.
pub type ConsensusParametersVersions =
    LocalMirror<fuel_core_storage::tables::ConsensusParametersVersions>;

/// Local mirror of the [`fuel_core_storage::tables::StateTransitionBytecodeVersions`] table.
pub type StateTransitionBytecodeVersions =
    LocalMirror<fuel_core_storage::tables::StateTransitionBytecodeVersions>;

/// Local mirror of the [`fuel_core_storage::tables::UploadedBytecodes`] table.
pub type UploadedBytecodes = LocalMirror<fuel_core_storage::tables::UploadedBytecodes>;

/// Local mirror of the [`fuel_core_storage::tables::BlobData`] table.
pub type BlobData = LocalMirror<fuel_core_storage::tables::BlobData>;

/// Wrapper over an existing table to allow implementing new foreign traits on it.
pub struct LocalMirror<Table>(core::marker::PhantomData<Table>);

impl<Table: Mappable> Mappable for LocalMirror<Table> {
    type Key = <Table as Mappable>::Key;
    type OwnedKey = <Table as Mappable>::OwnedKey;
    type OwnedValue = <Table as Mappable>::OwnedValue;
    type Value = <Table as Mappable>::Value;
}
