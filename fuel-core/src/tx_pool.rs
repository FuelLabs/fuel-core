use fuel_core_interfaces::{
    common::{
        fuel_vm::prelude::ProgramState,
        tai64::Tai64,
    },
    model::BlockId,
};
use serde::{
    Deserialize,
    Serialize,
};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum TransactionStatus {
    Submitted {
        time: Tai64,
    },
    Success {
        block_id: BlockId,
        time: Tai64,
        result: Option<ProgramState>,
    },
    SqueezedOut {
        reason: String,
    },
    Failed {
        block_id: BlockId,
        time: Tai64,
        reason: String,
        result: Option<ProgramState>,
    },
}
