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

use crate::merkle::Merklized;

pub type ContractsRawCode = Merklized<fuel_core_storage::tables::ContractsRawCode>;
pub type ContractsLatestUtxo = Merklized<fuel_core_storage::tables::ContractsLatestUtxo>;
pub type Coins = Merklized<fuel_core_storage::tables::Coins>;
pub type Messages = Merklized<fuel_core_storage::tables::Messages>;
pub type ProcessedTransactions =
    Merklized<fuel_core_storage::tables::ProcessedTransactions>;
pub type ConsensusParametersVersions =
    Merklized<fuel_core_storage::tables::ConsensusParametersVersions>;
pub type StateTransitionBytecodeVersions =
    Merklized<fuel_core_storage::tables::StateTransitionBytecodeVersions>;
pub type UploadedBytecodes = Merklized<fuel_core_storage::tables::UploadedBytecodes>;
pub type Blobs = Merklized<fuel_core_storage::tables::BlobData>;
