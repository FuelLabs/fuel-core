use fuel_core_interfaces::common::{fuel_tx::Bytes32, fuel_vm::prelude::ProgramState};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum TransactionStatus {
    Submitted {
        time: OffsetDateTime,
    },
    Success {
        block_id: Bytes32,
        time: OffsetDateTime,
        result: ProgramState,
    },
    Failed {
        block_id: Bytes32,
        time: OffsetDateTime,
        reason: String,
        result: Option<ProgramState>,
    },
}
