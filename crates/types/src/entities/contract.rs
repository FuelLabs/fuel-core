//! Contract entities

use crate::fuel_tx::{
    Salt,
    TxPointer,
};
use fuel_vm_private::fuel_tx::UtxoId;

/// Contains information related to the latest contract utxo
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub enum ContractUtxoInfo {
    /// V1 ContractUtxoInfo
    V1(ContractUtxoInfoV1),
}

impl Default for ContractUtxoInfo {
    fn default() -> Self {
        Self::V1(Default::default())
    }
}

impl ContractUtxoInfo {
    /// Get the Contract UTXO's id
    pub fn utxo_id(&self) -> &UtxoId {
        match self {
            ContractUtxoInfo::V1(info) => &info.utxo_id,
        }
    }

    /// Get the Contract UTXO's transaction pointer
    pub fn tx_pointer(&self) -> TxPointer {
        match self {
            ContractUtxoInfo::V1(info) => info.tx_pointer,
        }
    }
}

/// Version 1 of the ContractUtxoInfo
#[derive(Debug, Default, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ContractUtxoInfoV1 {
    /// the utxo id of the contract
    pub utxo_id: UtxoId,
    /// the tx pointer to the utxo
    pub tx_pointer: TxPointer,
}

impl From<(UtxoId, TxPointer)> for ContractUtxoInfoV1 {
    fn from((utxo_id, tx_pointer): (UtxoId, TxPointer)) -> Self {
        Self {
            utxo_id,
            tx_pointer,
        }
    }
}

/// Versioned type for storing information about a contract. Contract
/// information is off-chain data.
// TODO: Move ContractsInfoType to off-chain data storage https://github.com/FuelLabs/fuel-core/issues/1654
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub enum ContractsInfoType {
    /// V1 ContractsInfoType
    V1(ContractsInfoTypeV1),
}

impl ContractsInfoType {
    /// Get the contract salt
    pub fn salt(&self) -> &Salt {
        match self {
            ContractsInfoType::V1(info) => &info.salt,
        }
    }
}

/// Version 1 of the ContractsInfoType
#[derive(Debug, Default, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ContractsInfoTypeV1 {
    salt: Salt,
}

impl From<ContractsInfoTypeV1> for Salt {
    fn from(value: ContractsInfoTypeV1) -> Self {
        value.salt
    }
}

impl From<Salt> for ContractsInfoTypeV1 {
    fn from(salt: Salt) -> Self {
        Self { salt }
    }
}

impl From<Salt> for ContractsInfoType {
    fn from(salt: Salt) -> Self {
        ContractsInfoType::V1(salt.into())
    }
}
