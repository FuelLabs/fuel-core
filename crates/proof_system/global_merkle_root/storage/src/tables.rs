//! This module mirrors the tables from fuel-core-storage
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
use fuel_core_types::{
    blockchain::header::{
        ConsensusParametersVersion,
        StateTransitionBytecodeVersion,
    },
    entities::{
        coins::coin::CompressedCoin,
        contract::ContractUtxoInfo,
        Message,
    },
    fuel_tx::{
        BlobId,
        Bytes32,
        ConsensusParameters,
        ContractId,
        TxId,
        UtxoId,
    },
    fuel_types::Nonce,
    fuel_vm::{
        BlobBytes,
        Contract,
        UploadedBytecode,
    },
};

/// The storage table for contract's raw byte code.
pub struct ContractsRawCode;

impl Mappable for ContractsRawCode {
    type Key = Self::OwnedKey;
    type OwnedKey = ContractId;
    type OwnedValue = Contract;
    type Value = [u8];
}

/// The latest UTXO info of the contract. The contract's UTXO represents the unique id of the state.
/// After each transaction, old UTXO is consumed, and new UTXO is produced. UTXO is used as an
/// input to the next transaction related to the `ContractId` smart contract.
pub struct ContractsLatestUtxo;

impl Mappable for ContractsLatestUtxo {
    type Key = Self::OwnedKey;
    type OwnedKey = ContractId;
    /// The latest UTXO info
    type Value = Self::OwnedValue;
    type OwnedValue = ContractUtxoInfo;
}

/// The storage table of coins. Each [`CompressedCoin`]
/// is represented by unique `UtxoId`.
pub struct Coins;

impl Mappable for Coins {
    type Key = Self::OwnedKey;
    type OwnedKey = UtxoId;
    type Value = Self::OwnedValue;
    type OwnedValue = CompressedCoin;
}

/// The storage table of bridged Ethereum message.
pub struct Messages;

impl Mappable for Messages {
    type Key = Self::OwnedKey;
    type OwnedKey = Nonce;
    type Value = Self::OwnedValue;
    type OwnedValue = Message;
}

/// The storage table of processed transactions that were executed in the past.
/// The table helps to drop duplicated transactions.
pub struct ProcessedTransactions;

impl Mappable for ProcessedTransactions {
    type Key = Self::OwnedKey;
    type OwnedKey = TxId;
    type Value = Self::OwnedValue;
    type OwnedValue = ();
}

/// The storage table of consensus parameters.
pub struct ConsensusParametersVersions;

impl Mappable for ConsensusParametersVersions {
    type Key = Self::OwnedKey;
    type OwnedKey = ConsensusParametersVersion;
    type Value = Self::OwnedValue;
    type OwnedValue = ConsensusParameters;
}

/// The storage table of state transition bytecodes.
pub struct StateTransitionBytecodeVersions;

impl Mappable for StateTransitionBytecodeVersions {
    type Key = Self::OwnedKey;
    type OwnedKey = StateTransitionBytecodeVersion;
    type Value = Self::OwnedValue;
    /// The Merkle root of the bytecode from the [`UploadedBytecodes`].
    type OwnedValue = Bytes32;
}

/// The storage table for uploaded bytecode.
pub struct UploadedBytecodes;

impl Mappable for UploadedBytecodes {
    /// The key is a Merkle root of the bytecode.
    type Key = Self::OwnedKey;
    type OwnedKey = Bytes32;
    type OwnedValue = UploadedBytecode;
    type Value = Self::OwnedValue;
}

/// The storage table for blob data bytes.
pub struct BlobData;

impl Mappable for BlobData {
    type Key = Self::OwnedKey;
    type OwnedKey = BlobId;
    type OwnedValue = BlobBytes;
    type Value = [u8];
}
