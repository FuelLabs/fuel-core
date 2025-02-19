//! Storage types and update logic for the state root service

#![cfg_attr(not(feature = "std"), no_std)]
#![deny(clippy::arithmetic_side_effects)]
#![deny(clippy::cast_possible_truncation)]
#![deny(unused_crate_dependencies)]
#![deny(missing_docs)]
#![deny(warnings)]

extern crate alloc;

/// Table definitions
pub mod column;

/// Merkleization primitives
pub mod merkle;

/// Column implementations
pub mod structured_storage;

/// Update logic
pub mod update;

/// Test helpers
#[cfg(feature = "test-helpers")]
pub mod test_helpers;

/// Hack, see how it's used in `test_helpers`
#[cfg(feature = "test-helpers")]
pub struct Dummy;

use crate::merkle::Merkleized;

/// Merkleized `ContractsRawCode` table.
pub type ContractsRawCode = Merkleized<fuel_core_storage::tables::ContractsRawCode>;
/// Merkleized `ContractsLatestUtxo` table.
pub type ContractsLatestUtxo = Merkleized<fuel_core_storage::tables::ContractsLatestUtxo>;
/// Merkleized `Coins` table.
pub type Coins = Merkleized<fuel_core_storage::tables::Coins>;
/// Merkleized `Messages` table.
pub type Messages = Merkleized<fuel_core_storage::tables::Messages>;
/// Merkleized `ProcessedTransactions` table.
pub type ProcessedTransactions =
    Merkleized<fuel_core_storage::tables::ProcessedTransactions>;
/// Merkleized `ConsensusParametersVersions` table.
pub type ConsensusParametersVersions =
    Merkleized<fuel_core_storage::tables::ConsensusParametersVersions>;
/// Merkleized `StateTransitionBytecodeVersions` table.
pub type StateTransitionBytecodeVersions =
    Merkleized<fuel_core_storage::tables::StateTransitionBytecodeVersions>;
/// Merkleized `UploadedBytecodes` table.
pub type UploadedBytecodes = Merkleized<fuel_core_storage::tables::UploadedBytecodes>;
/// Merkleized `Blobs` table.
pub type Blobs = Merkleized<fuel_core_storage::tables::BlobData>;
