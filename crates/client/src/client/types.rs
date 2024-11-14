pub mod balance;
pub mod blob;
pub mod block;
pub mod chain_info;
pub mod coins;
pub mod contract;
pub mod gas_costs;
pub mod upgrades;

pub mod gas_price;
pub mod merkle_proof;
pub mod message;
pub mod node_info;

pub use balance::Balance;
pub use blob::Blob;
pub use block::{Block, Consensus};
pub use chain_info::ChainInfo;
pub use coins::{Coin, CoinType, MessageCoin};
pub use contract::{Contract, ContractBalance};
pub use gas_costs::{DependentCost, GasCosts};
pub use merkle_proof::MerkleProof;
pub use message::{Message, MessageProof};
pub use node_info::NodeInfo;

use crate::client::schema::{
    relayed_tx::RelayedTransactionStatus as SchemaRelayedTransactionStatus,
    tx::{
        OpaqueTransactionWithStatus, StatusWithTransaction as SchemaStatusWithTx,
        TransactionStatus as SchemaTxStatus,
    },
    ConversionError,
};
use fuel_core_types::{
    fuel_tx::{Receipt, Transaction},
    fuel_types::{canonical::Deserialize, BlockHeight},
    fuel_vm::ProgramState,
};
use tai64::Tai64;

pub mod primitives {
    pub use fuel_core_types::{
        fuel_crypto::{PublicKey, Signature},
        fuel_tx::UtxoId,
        fuel_types::{
            Address, AssetId, BlobId, Bytes32, Bytes64, ChainId, ContractId, MessageId,
            Nonce, Salt,
        },
    };

    pub type BlockId = Bytes32;
    pub type Hash = Bytes32;
    pub type Bytes = Vec<u8>;
    pub type MerkleRoot = Bytes32;
    pub type TransactionId = Bytes32;
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TransactionResponse {
    pub transaction: TransactionType,
    pub status: TransactionStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StatusWithTransactionResponse {
    pub status: StatusWithTransaction,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TransactionStatus {
    Submitted {
        submitted_at: Tai64,
    },
    Success {
        block_height: BlockHeight,
        time: Tai64,
        program_state: Option<ProgramState>,
        receipts: Vec<Receipt>,
        total_gas: u64,
        total_fee: u64,
    },
    SqueezedOut {
        reason: String,
    },
    Failure {
        block_height: BlockHeight,
        time: Tai64,
        reason: String,
        program_state: Option<ProgramState>,
        receipts: Vec<Receipt>,
        total_gas: u64,
        total_fee: u64,
    },
}

impl TryFrom<SchemaTxStatus> for TransactionStatus {
    type Error = ConversionError;

    fn try_from(status: SchemaTxStatus) -> Result<Self, Self::Error> {
        Ok(match status {
            SchemaTxStatus::SubmittedStatus(s) => TransactionStatus::Submitted {
                submitted_at: s.time.0,
            },
            SchemaTxStatus::SuccessStatus(s) => TransactionStatus::Success {
                block_height: s.block_height.into(),
                time: s.time.0,
                program_state: s.program_state.map(TryInto::try_into).transpose()?,
                receipts: s
                    .receipts
                    .into_iter()
                    .map(TryInto::try_into)
                    .collect::<Result<Vec<_>, _>>()?,
                total_gas: s.total_gas.0,
                total_fee: s.total_fee.0,
            },
            SchemaTxStatus::FailureStatus(s) => TransactionStatus::Failure {
                block_height: s.block_height.into(),
                time: s.time.0,
                reason: s.reason,
                program_state: s.program_state.map(TryInto::try_into).transpose()?,
                receipts: s
                    .receipts
                    .into_iter()
                    .map(TryInto::try_into)
                    .collect::<Result<Vec<_>, _>>()?,
                total_gas: s.total_gas.0,
                total_fee: s.total_fee.0,
            },
            SchemaTxStatus::SqueezedOutStatus(s) => {
                TransactionStatus::SqueezedOut { reason: s.reason }
            }
            SchemaTxStatus::Unknown => {
                return Err(Self::Error::UnknownVariant("SchemaTxStatus"))
            }
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum StatusWithTransaction {
    Submitted {
        submitted_at: Tai64,
    },
    Success {
        transaction: Transaction,
        block_height: BlockHeight,
        time: Tai64,
        program_state: Option<ProgramState>,
        receipts: Vec<Receipt>,
        total_gas: u64,
        total_fee: u64,
    },
    SqueezedOut {
        reason: String,
    },
    Failure {
        transaction: Transaction,
        block_height: BlockHeight,
        time: Tai64,
        reason: String,
        program_state: Option<ProgramState>,
        receipts: Vec<Receipt>,
        total_gas: u64,
        total_fee: u64,
    },
}

impl TryFrom<SchemaStatusWithTx> for StatusWithTransaction {
    type Error = ConversionError;

    fn try_from(status: SchemaStatusWithTx) -> Result<Self, Self::Error> {
        Ok(match status {
            SchemaStatusWithTx::SubmittedStatus(s) => StatusWithTransaction::Submitted {
                submitted_at: s.time.0,
            },
            SchemaStatusWithTx::SuccessStatus(s) => StatusWithTransaction::Success {
                transaction: s.transaction.try_into()?,
                block_height: s.block_height.into(),
                time: s.time.0,
                program_state: s.program_state.map(TryInto::try_into).transpose()?,
                receipts: s
                    .receipts
                    .into_iter()
                    .map(TryInto::try_into)
                    .collect::<Result<Vec<_>, _>>()?,
                total_gas: s.total_gas.0,
                total_fee: s.total_fee.0,
            },
            SchemaStatusWithTx::FailureStatus(s) => StatusWithTransaction::Failure {
                transaction: s.transaction.try_into()?,
                block_height: s.block_height.into(),
                time: s.time.0,
                reason: s.reason,
                program_state: s.program_state.map(TryInto::try_into).transpose()?,
                receipts: s
                    .receipts
                    .into_iter()
                    .map(TryInto::try_into)
                    .collect::<Result<Vec<_>, _>>()?,
                total_gas: s.total_gas.0,
                total_fee: s.total_fee.0,
            },
            SchemaStatusWithTx::SqueezedOutStatus(s) => {
                StatusWithTransaction::SqueezedOut { reason: s.reason }
            }
            SchemaStatusWithTx::Unknown => {
                return Err(Self::Error::UnknownVariant("SchemaTxStatus"))
            }
        })
    }
}

impl TryFrom<OpaqueTransactionWithStatus> for TransactionResponse {
    type Error = ConversionError;

    fn try_from(value: OpaqueTransactionWithStatus) -> Result<Self, Self::Error> {
        let bytes = value.raw_payload.0 .0;
        let tx: TransactionType = Transaction::from_bytes(bytes.as_slice())
            .map(Into::into)
            .unwrap_or(TransactionType::Unknown);
        let status = value
            .status
            .ok_or_else(|| ConversionError::MissingField("status".to_string()))?
            .try_into()?;

        Ok(Self {
            transaction: tx,
            status,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelayedTransactionStatus {
    Failed {
        block_height: BlockHeight,
        failure: String,
    },
}

impl TryFrom<SchemaRelayedTransactionStatus> for RelayedTransactionStatus {
    type Error = ConversionError;

    fn try_from(status: SchemaRelayedTransactionStatus) -> Result<Self, Self::Error> {
        Ok(match status {
            SchemaRelayedTransactionStatus::Failed(s) => {
                RelayedTransactionStatus::Failed {
                    block_height: s.block_height.into(),
                    failure: s.failure,
                }
            }
            SchemaRelayedTransactionStatus::Unknown => {
                return Err(Self::Error::UnknownVariant(
                    "SchemaRelayedTransactionStatus",
                ));
            }
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[allow(clippy::large_enum_variant)]
pub enum TransactionType {
    Known(Transaction),
    Unknown,
}

impl From<Transaction> for TransactionType {
    fn from(value: Transaction) -> Self {
        Self::Known(value)
    }
}

impl TryFrom<TransactionType> for Transaction {
    type Error = ConversionError;

    fn try_from(value: TransactionType) -> Result<Self, ConversionError> {
        match value {
            TransactionType::Known(tx) => Ok(tx),
            TransactionType::Unknown => {
                Err(ConversionError::UnknownVariant("Transaction"))
            }
        }
    }
}
