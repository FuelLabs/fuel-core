//! Contract entities

use crate::fuel_tx::TxPointer;
use fuel_vm_private::fuel_tx::UtxoId;

/// Contains information related to the latest contract utxo
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Default, Clone, PartialEq)]
pub struct ContractUtxoInfo {
    /// the utxo id of the contract
    pub utxo_id: UtxoId,
    /// the tx pointer to the utxo
    pub tx_pointer: TxPointer,
}
