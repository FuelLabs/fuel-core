#![cfg_attr(not(feature = "std"), no_std)]
#![deny(clippy::arithmetic_side_effects)]
#![deny(clippy::cast_possible_truncation)]
#![deny(unused_crate_dependencies)]
#![deny(warnings)]

extern crate alloc;

pub mod column;
pub mod merkle;
pub mod structured_storage;
pub mod update;

use crate::merkle::Merkleized;

pub type ContractsRawCode = Merkleized<fuel_core_storage::tables::ContractsRawCode>;
pub type ContractsLatestUtxo = Merkleized<fuel_core_storage::tables::ContractsLatestUtxo>;
pub type Coins = Merkleized<fuel_core_storage::tables::Coins>;
pub type Messages = Merkleized<fuel_core_storage::tables::Messages>;
pub type ProcessedTransactions =
    Merkleized<fuel_core_storage::tables::ProcessedTransactions>;
pub type ConsensusParametersVersions =
    Merkleized<fuel_core_storage::tables::ConsensusParametersVersions>;
pub type StateTransitionBytecodeVersions =
    Merkleized<fuel_core_storage::tables::StateTransitionBytecodeVersions>;
pub type UploadedBytecodes = Merkleized<fuel_core_storage::tables::UploadedBytecodes>;
pub type Blobs = Merkleized<fuel_core_storage::tables::BlobData>;
