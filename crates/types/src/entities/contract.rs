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

/// Versioned enum for holding information about a contract
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub enum ContractsInfoType {
    V1(ContractsInfoTypeV1),
}

#[derive(Debug, Default, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ContractsInfoTypeV1 {
    pub salt: Salt,
}
