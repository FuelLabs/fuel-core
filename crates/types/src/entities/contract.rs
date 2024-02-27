//! Contract entities

use crate::fuel_tx::{
    Salt,
    TxPointer,
};
use fuel_vm_private::fuel_tx::UtxoId;

/// Contains information related to the latest contract utxo
#[derive(Debug, Default, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ContractUtxoInfo {
    /// the utxo id of the contract
    pub utxo_id: UtxoId,
    /// the tx pointer to the utxo
    pub tx_pointer: TxPointer,
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

impl From<Salt> for ContractsInfoTypeV1 {
    fn from(salt: Salt) -> Self {
        Self { salt }
    }
}
