use chrono::{DateTime, Utc};
use fuel_tx::Bytes32;
//use fuel_txpool::Service as TxPoolService;
use fuel_vm::prelude::ProgramState;
//use itertools::Itertools;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum TransactionStatus {
    Submitted {
        time: DateTime<Utc>,
    },
    Success {
        block_id: Bytes32,
        time: DateTime<Utc>,
        result: ProgramState,
    },
    Failed {
        block_id: Bytes32,
        time: DateTime<Utc>,
        reason: String,
        result: Option<ProgramState>,
    },
}
