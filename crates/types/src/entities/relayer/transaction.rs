//! Relayed (forced) transaction entity types

use crate::{
    blockchain::primitives::DaBlockHeight,
    fuel_crypto,
    fuel_types::{
        BlockHeight,
        Bytes32,
        Nonce,
    },
};

#[cfg(feature = "alloc")]
use alloc::{
    string::String,
    vec::Vec,
};

/// Transaction sent from the DA layer to fuel by the relayer
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum RelayedTransaction {
    /// V1 version of the relayed transaction
    V1(RelayedTransactionV1),
}

impl Default for RelayedTransaction {
    fn default() -> Self {
        Self::V1(Default::default())
    }
}

/// The V1 version of the relayed transaction
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct RelayedTransactionV1 {
    /// Unique nonce for this FTI event
    pub nonce: Nonce,
    /// The max gas that this transaction can consume
    pub max_gas: u64,
    /// The serialized transaction transmitted from the bridge
    pub serialized_transaction: Vec<u8>,
    /// The block height from the parent da layer that originated this transaction
    pub da_height: DaBlockHeight,
}

/// The hash of a relayed transaction
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    derive_more::Display,
    derive_more::From,
    derive_more::Into,
)]
pub struct RelayedTransactionId(Bytes32);

impl RelayedTransaction {
    /// The hash of the relayed transaction
    pub fn id(&self) -> RelayedTransactionId {
        match &self {
            RelayedTransaction::V1(tx) => tx.id(),
        }
    }

    /// The unique nonce associated with this relayed transaction
    pub fn nonce(&self) -> Nonce {
        match &self {
            RelayedTransaction::V1(tx) => tx.nonce,
        }
    }

    #[cfg(any(test, feature = "test-helpers"))]
    /// Set the nonce
    pub fn set_nonce(&mut self, nonce: Nonce) {
        match self {
            RelayedTransaction::V1(transaction) => {
                transaction.nonce = nonce;
            }
        }
    }

    /// Get the DA height that originated this transaction from L1
    pub fn da_height(&self) -> DaBlockHeight {
        match self {
            RelayedTransaction::V1(transaction) => transaction.da_height,
        }
    }

    /// Get the max gas
    pub fn max_gas(&self) -> u64 {
        match self {
            RelayedTransaction::V1(transaction) => transaction.max_gas,
        }
    }

    #[cfg(any(test, feature = "test-helpers"))]
    /// Set the da height
    pub fn set_da_height(&mut self, height: DaBlockHeight) {
        match self {
            RelayedTransaction::V1(transaction) => {
                transaction.da_height = height;
            }
        }
    }

    #[cfg(any(test, feature = "test-helpers"))]
    /// Set the max gas
    pub fn set_max_gas(&mut self, max_gas: u64) {
        match self {
            RelayedTransaction::V1(transaction) => {
                transaction.max_gas = max_gas;
            }
        }
    }

    /// Get the canonically serialized transaction
    pub fn serialized_transaction(&self) -> &[u8] {
        match self {
            RelayedTransaction::V1(transaction) => &transaction.serialized_transaction,
        }
    }

    #[cfg(any(test, feature = "test-helpers"))]
    /// Set the serialized transaction bytes
    pub fn set_serialized_transaction(&mut self, serialized_bytes: Vec<u8>) {
        match self {
            RelayedTransaction::V1(transaction) => {
                transaction.serialized_transaction = serialized_bytes;
            }
        }
    }
}

impl RelayedTransactionV1 {
    /// The hash of the relayed transaction (max_gas (big endian) || serialized_transaction)
    pub fn id(&self) -> RelayedTransactionId {
        let hasher = fuel_crypto::Hasher::default()
            .chain(self.nonce.as_slice())
            .chain(self.max_gas.to_be_bytes())
            .chain(self.serialized_transaction.as_slice());
        RelayedTransactionId((*hasher.finalize()).into())
    }
}

impl From<RelayedTransactionV1> for RelayedTransaction {
    fn from(relayed_transaction: RelayedTransactionV1) -> Self {
        RelayedTransaction::V1(relayed_transaction)
    }
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Debug, PartialEq, Eq)]
/// Potential states for the relayed transaction
pub enum RelayedTransactionStatus {
    /// Transaction was included in a block, but the execution was reverted
    Failed {
        /// The height of the block that processed this transaction
        block_height: BlockHeight,
        /// The actual failure reason for why the forced transaction was not included
        failure: String,
    },
}
