use crate::client::schema::tx::{OpaqueTransaction, TransactionStatus as SchemaTxStatus};
use crate::client::schema::ConversionError;
use chrono::{DateTime, Utc};
use fuel_tx::Transaction;
use fuel_types::bytes::Deserializable;
use fuel_vm::prelude::ProgramState;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionResponse {
    pub transaction: Transaction,
    pub status: TransactionStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransactionStatus {
    Submitted {
        submitted_at: DateTime<Utc>,
    },
    Success {
        block_id: String,
        time: DateTime<Utc>,
        program_state: ProgramState,
    },
    Failure {
        block_id: String,
        time: DateTime<Utc>,
        reason: String,
        program_state: Option<ProgramState>,
    },
}

impl TryFrom<SchemaTxStatus> for TransactionStatus {
    type Error = ConversionError;

    fn try_from(status: SchemaTxStatus) -> Result<Self, Self::Error> {
        Ok(match status {
            SchemaTxStatus::SubmittedStatus(s) => TransactionStatus::Submitted {
                submitted_at: s.time,
            },
            SchemaTxStatus::SuccessStatus(s) => TransactionStatus::Success {
                block_id: s.block_id.0.to_string(),
                time: s.time,
                program_state: s.program_state.try_into()?,
            },
            SchemaTxStatus::FailureStatus(s) => TransactionStatus::Failure {
                block_id: s.block_id.0.to_string(),
                time: s.time,
                reason: s.reason,
                program_state: s.program_state.map(TryInto::try_into).transpose()?,
            },
        })
    }
}

impl TryFrom<OpaqueTransaction> for TransactionResponse {
    type Error = ConversionError;

    fn try_from(value: OpaqueTransaction) -> Result<Self, Self::Error> {
        let bytes = value.raw_payload.0 .0;
        let tx: fuel_tx::Transaction = fuel_tx::Transaction::from_bytes(bytes.as_slice())
            .map_err(ConversionError::TransactionFromBytesError)?;
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
