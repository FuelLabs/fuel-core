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

/// Mappable tables
pub mod tables;

/// Merkleization primitives
pub mod merkle;

/// Column implementations
pub mod structured_storage;

/// Update logic
pub mod update;

/// Merkle root computation logic
pub mod compute;

/// Error type
pub mod error;

pub use error::{
    Error,
    Result,
};

/// Test helpers
#[cfg(feature = "test-helpers")]
pub mod test_helpers;

/// Hack, see how it's used in `test_helpers`
#[cfg(feature = "test-helpers")]
pub struct Dummy;

use crate::merkle::Merkleized;

/// Merkleized `ContractsRawCode` table.
pub type ContractsRawCode = Merkleized<tables::ContractsRawCode>;
/// Merkleized `ContractsLatestUtxo` table.
pub type ContractsLatestUtxo = Merkleized<tables::ContractsLatestUtxo>;
/// Merkleized `Coins` table.
pub type Coins = Merkleized<tables::Coins>;
/// Merkleized `Messages` table.
pub type Messages = Merkleized<tables::Messages>;
/// Merkleized `ProcessedTransactions` table.
pub type ProcessedTransactions = Merkleized<tables::ProcessedTransactions>;
/// Merkleized `ConsensusParametersVersions` table.
pub type ConsensusParametersVersions = Merkleized<tables::ConsensusParametersVersions>;
/// Merkleized `StateTransitionBytecodeVersions` table.
pub type StateTransitionBytecodeVersions =
    Merkleized<tables::StateTransitionBytecodeVersions>;
/// Merkleized `UploadedBytecodes` table.
pub type UploadedBytecodes = Merkleized<tables::UploadedBytecodes>;
/// Merkleized `Blobs` table.
pub type Blobs = Merkleized<tables::BlobData>;
