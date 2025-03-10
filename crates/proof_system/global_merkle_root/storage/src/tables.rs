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

/// The storage table for contract's raw byte code.
pub type ContractsRawCode = LocalMirror<fuel_core_storage::tables::ContractsRawCode>;

/// The latest UTXO info of the contract. The contract's UTXO represents the unique id of the state.
/// After each transaction, old UTXO is consumed, and new UTXO is produced. UTXO is used as an
/// input to the next transaction related to the `ContractId` smart contract.
pub type ContractsLatestUtxo =
    LocalMirror<fuel_core_storage::tables::ContractsLatestUtxo>;

/// The storage table of coins. Each [`CompressedCoin`]
/// is represented by unique `UtxoId`.
pub type Coins = LocalMirror<fuel_core_storage::tables::Coins>;

/// The storage table of bridged Ethereum message.
pub type Messages = LocalMirror<fuel_core_storage::tables::Messages>;

/// The storage table of processed transactions that were executed in the past.
/// The table helps to drop duplicated transactions.
pub type ProcessedTransactions =
    LocalMirror<fuel_core_storage::tables::ProcessedTransactions>;

/// The storage table of consensus parameters.
pub type ConsensusParametersVersions =
    LocalMirror<fuel_core_storage::tables::ConsensusParametersVersions>;

/// The storage table of state transition bytecodes.
pub type StateTransitionBytecodeVersions =
    LocalMirror<fuel_core_storage::tables::StateTransitionBytecodeVersions>;

/// The storage table for uploaded bytecode.
pub type UploadedBytecodes = LocalMirror<fuel_core_storage::tables::UploadedBytecodes>;

/// The storage table for blob data bytes.
pub type BlobData = LocalMirror<fuel_core_storage::tables::BlobData>;

/// Wrapper over an existing table to allow implementing new foreign traits on it.
pub struct LocalMirror<Table>(core::marker::PhantomData<Table>);

impl<Table: Mappable> Mappable for LocalMirror<Table> {
    type Key = <Table as Mappable>::Key;
    type OwnedKey = <Table as Mappable>::OwnedKey;
    type OwnedValue = <Table as Mappable>::OwnedValue;
    type Value = <Table as Mappable>::Value;
}
