//! Types for interoperability with the txpool service

use crate::{
    blockchain::header::ConsensusParametersVersion,
    fuel_asm::Word,
    fuel_tx::{
        Blob,
        Cacheable,
        Chargeable,
        Create,
        Input,
        Output,
        Script,
        Transaction,
        TxId,
        Upgrade,
        Upload,
        UtxoId,
        field::{
            Inputs,
            Outputs,
            ScriptGasLimit,
            Tip,
        },
    },
    fuel_vm::checked_transaction::Checked,
};
use fuel_vm_private::{
    checked_transaction::CheckedTransaction,
    fuel_types::BlockHeight,
    prelude::field::Expiration,
};
use std::sync::Arc;

/// Pool transaction wrapped in an Arc for thread-safe sharing
pub type ArcPoolTx = Arc<PoolTransaction>;

/// Metadata for the transaction pool.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct Metadata {
    version: ConsensusParametersVersion,
    size: Option<usize>,
    max_gas_price: Word,
    #[cfg(feature = "test-helpers")]
    max_gas: Option<Word>,
    #[cfg(feature = "test-helpers")]
    tx_id: Option<TxId>,
}

impl Metadata {
    /// Create a new metadata for the transaction from the pool.
    pub fn new(
        version: ConsensusParametersVersion,
        size: usize,
        max_gas_price: Word,
    ) -> Self {
        Self {
            version,
            size: Some(size),
            max_gas_price,
            #[cfg(feature = "test-helpers")]
            max_gas: None,
            #[cfg(feature = "test-helpers")]
            tx_id: None,
        }
    }

    /// Create a new test metadata for the transaction from the pool.
    #[cfg(feature = "test-helpers")]
    pub fn new_test(
        version: ConsensusParametersVersion,
        max_gas: Option<u64>,
        tx_id: Option<TxId>,
    ) -> Self {
        Self {
            version,
            size: None,
            max_gas_price: 0,
            max_gas,
            tx_id,
        }
    }

    /// Returns the max gas price for the transaction.
    pub fn max_gas_price(&self) -> Word {
        self.max_gas_price
    }
}

/// Transaction type used by the transaction pool.
/// Not all `fuel_tx::Transaction` variants are supported by the txpool.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum PoolTransaction {
    /// Script
    Script(Checked<Script>, Metadata),
    /// Create
    Create(Checked<Create>, Metadata),
    /// Upgrade
    Upgrade(Checked<Upgrade>, Metadata),
    /// Upload
    Upload(Checked<Upload>, Metadata),
    /// Blob
    Blob(Checked<Blob>, Metadata),
}

impl PoolTransaction {
    /// Returns the version of the consensus parameters used to create `Checked` transaction.
    pub fn used_consensus_parameters_version(&self) -> ConsensusParametersVersion {
        self.metadata_inner().version
    }

    /// Used for accounting purposes when charging byte based fees.
    pub fn metered_bytes_size(&self) -> usize {
        if let Some(size) = self.metadata_inner().size {
            size
        } else {
            match self {
                PoolTransaction::Script(tx, _) => tx.transaction().metered_bytes_size(),
                PoolTransaction::Create(tx, _) => tx.transaction().metered_bytes_size(),
                PoolTransaction::Upgrade(tx, _) => tx.transaction().metered_bytes_size(),
                PoolTransaction::Upload(tx, _) => tx.transaction().metered_bytes_size(),
                PoolTransaction::Blob(tx, _) => tx.transaction().metered_bytes_size(),
            }
        }
    }

    /// Returns the maximum gas price for the transaction.
    pub fn max_gas_price(&self) -> Word {
        match self {
            PoolTransaction::Script(_, metadata) => metadata.max_gas_price,
            PoolTransaction::Create(_, metadata) => metadata.max_gas_price,
            PoolTransaction::Upgrade(_, metadata) => metadata.max_gas_price,
            PoolTransaction::Upload(_, metadata) => metadata.max_gas_price,
            PoolTransaction::Blob(_, metadata) => metadata.max_gas_price,
        }
    }

    /// Returns the expiration block for a transaction.
    pub fn expiration(&self) -> BlockHeight {
        match self {
            PoolTransaction::Script(tx, _) => tx.transaction().expiration(),
            PoolTransaction::Create(tx, _) => tx.transaction().expiration(),
            PoolTransaction::Upgrade(tx, _) => tx.transaction().expiration(),
            PoolTransaction::Upload(tx, _) => tx.transaction().expiration(),
            PoolTransaction::Blob(tx, _) => tx.transaction().expiration(),
        }
    }

    #[cfg(feature = "test-helpers")]
    fn id_inner(&self) -> TxId {
        match self {
            PoolTransaction::Script(tx, _) => tx.id(),
            PoolTransaction::Create(tx, _) => tx.id(),
            PoolTransaction::Upgrade(tx, _) => tx.id(),
            PoolTransaction::Upload(tx, _) => tx.id(),
            PoolTransaction::Blob(tx, _) => tx.id(),
        }
    }

    #[cfg(feature = "test-helpers")]
    fn max_gas_inner(&self) -> Word {
        match self {
            PoolTransaction::Script(tx, _) => tx.metadata().max_gas,
            PoolTransaction::Create(tx, _) => tx.metadata().max_gas,
            PoolTransaction::Upgrade(tx, _) => tx.metadata().max_gas,
            PoolTransaction::Upload(tx, _) => tx.metadata().max_gas,
            PoolTransaction::Blob(tx, _) => tx.metadata().max_gas,
        }
    }

    fn metadata_inner(&self) -> &Metadata {
        match self {
            PoolTransaction::Script(_, metadata) => metadata,
            PoolTransaction::Create(_, metadata) => metadata,
            PoolTransaction::Upgrade(_, metadata) => metadata,
            PoolTransaction::Upload(_, metadata) => metadata,
            PoolTransaction::Blob(_, metadata) => metadata,
        }
    }

    /// Returns the transaction ID
    #[cfg(not(feature = "test-helpers"))]
    pub fn id(&self) -> TxId {
        match self {
            PoolTransaction::Script(tx, _) => tx.id(),
            PoolTransaction::Create(tx, _) => tx.id(),
            PoolTransaction::Upgrade(tx, _) => tx.id(),
            PoolTransaction::Upload(tx, _) => tx.id(),
            PoolTransaction::Blob(tx, _) => tx.id(),
        }
    }

    /// Returns the transaction ID
    #[cfg(feature = "test-helpers")]
    pub fn id(&self) -> TxId {
        if let Some(tx_id) = self.metadata_inner().tx_id {
            tx_id
        } else {
            self.id_inner()
        }
    }

    /// Returns the maximum amount of gas that the transaction can consume.
    #[cfg(not(feature = "test-helpers"))]
    pub fn max_gas(&self) -> Word {
        match self {
            PoolTransaction::Script(tx, _) => tx.metadata().max_gas,
            PoolTransaction::Create(tx, _) => tx.metadata().max_gas,
            PoolTransaction::Upgrade(tx, _) => tx.metadata().max_gas,
            PoolTransaction::Upload(tx, _) => tx.metadata().max_gas,
            PoolTransaction::Blob(tx, _) => tx.metadata().max_gas,
        }
    }

    /// Returns the maximum amount of gas that the transaction can consume.
    #[cfg(feature = "test-helpers")]
    pub fn max_gas(&self) -> Word {
        if let Some(max_gas) = self.metadata_inner().max_gas {
            max_gas
        } else {
            self.max_gas_inner()
        }
    }

    /// Returns true if the transaction is a blob.
    pub fn is_blob(&self) -> bool {
        matches!(self, PoolTransaction::Blob(_, _))
    }
}

#[allow(missing_docs)]
impl PoolTransaction {
    pub fn script_gas_limit(&self) -> Option<Word> {
        match self {
            PoolTransaction::Script(script, _) => {
                Some(*script.transaction().script_gas_limit())
            }
            PoolTransaction::Create(_, _) => None,
            PoolTransaction::Upgrade(_, _) => None,
            PoolTransaction::Upload(_, _) => None,
            PoolTransaction::Blob(_, _) => None,
        }
    }

    pub fn tip(&self) -> Word {
        match self {
            Self::Script(tx, _) => tx.transaction().tip(),
            Self::Create(tx, _) => tx.transaction().tip(),
            Self::Upload(tx, _) => tx.transaction().tip(),
            Self::Upgrade(tx, _) => tx.transaction().tip(),
            Self::Blob(tx, _) => tx.transaction().tip(),
        }
    }

    pub fn is_computed(&self) -> bool {
        match self {
            PoolTransaction::Script(tx, _) => tx.transaction().is_computed(),
            PoolTransaction::Create(tx, _) => tx.transaction().is_computed(),
            PoolTransaction::Upgrade(tx, _) => tx.transaction().is_computed(),
            PoolTransaction::Upload(tx, _) => tx.transaction().is_computed(),
            PoolTransaction::Blob(tx, _) => tx.transaction().is_computed(),
        }
    }

    pub fn inputs(&self) -> &Vec<Input> {
        match self {
            PoolTransaction::Script(tx, _) => tx.transaction().inputs(),
            PoolTransaction::Create(tx, _) => tx.transaction().inputs(),
            PoolTransaction::Upgrade(tx, _) => tx.transaction().inputs(),
            PoolTransaction::Upload(tx, _) => tx.transaction().inputs(),
            PoolTransaction::Blob(tx, _) => tx.transaction().inputs(),
        }
    }

    pub fn outputs(&self) -> &Vec<Output> {
        match self {
            PoolTransaction::Script(tx, _) => tx.transaction().outputs(),
            PoolTransaction::Create(tx, _) => tx.transaction().outputs(),
            PoolTransaction::Upgrade(tx, _) => tx.transaction().outputs(),
            PoolTransaction::Upload(tx, _) => tx.transaction().outputs(),
            PoolTransaction::Blob(tx, _) => tx.transaction().outputs(),
        }
    }

    pub fn utxo_ids_with_outputs(&self) -> impl Iterator<Item = (UtxoId, &Output)> {
        utxo_ids_with_outputs(self.outputs().iter(), self.id())
    }
}

/// Helper function to create an iterator of utxo ids with outputs.
pub fn utxo_ids_with_outputs<'a, I>(
    outputs: I,
    tx_id: TxId,
) -> impl Iterator<Item = (UtxoId, &'a Output)>
where
    I: Iterator<Item = &'a Output>,
{
    outputs.enumerate().filter_map(move |(index, output)| {
        let output_index = u16::try_from(index).ok()?;
        let utxo_id = UtxoId::new(tx_id, output_index);
        Some((utxo_id, output))
    })
}

impl From<PoolTransaction> for CheckedTransaction {
    fn from(tx: PoolTransaction) -> Self {
        match tx {
            PoolTransaction::Script(tx, _) => CheckedTransaction::Script(tx),
            PoolTransaction::Create(tx, _) => CheckedTransaction::Create(tx),
            PoolTransaction::Upgrade(tx, _) => CheckedTransaction::Upgrade(tx),
            PoolTransaction::Upload(tx, _) => CheckedTransaction::Upload(tx),
            PoolTransaction::Blob(tx, _) => CheckedTransaction::Blob(tx),
        }
    }
}

impl From<&PoolTransaction> for Transaction {
    fn from(tx: &PoolTransaction) -> Self {
        match tx {
            PoolTransaction::Script(tx, _) => {
                Transaction::Script(tx.transaction().clone())
            }
            PoolTransaction::Create(tx, _) => {
                Transaction::Create(tx.transaction().clone())
            }
            PoolTransaction::Upgrade(tx, _) => {
                Transaction::Upgrade(tx.transaction().clone())
            }
            PoolTransaction::Upload(tx, _) => {
                Transaction::Upload(tx.transaction().clone())
            }
            PoolTransaction::Blob(tx, _) => Transaction::Blob(tx.transaction().clone()),
        }
    }
}

impl From<&PoolTransaction> for CheckedTransaction {
    fn from(tx: &PoolTransaction) -> Self {
        match tx {
            PoolTransaction::Script(tx, _) => CheckedTransaction::Script(tx.clone()),
            PoolTransaction::Create(tx, _) => CheckedTransaction::Create(tx.clone()),
            PoolTransaction::Upgrade(tx, _) => CheckedTransaction::Upgrade(tx.clone()),
            PoolTransaction::Upload(tx, _) => CheckedTransaction::Upload(tx.clone()),
            PoolTransaction::Blob(tx, _) => CheckedTransaction::Blob(tx.clone()),
        }
    }
}
