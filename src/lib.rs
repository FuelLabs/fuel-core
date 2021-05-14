#![feature(arbitrary_enum_discriminant)]
#![feature(is_sorted)]

pub mod bytes;
pub mod consts;
pub mod crypto;
pub mod interpreter;
pub mod opcodes;
pub mod transaction;
pub mod types;

pub mod prelude {
    pub use crate::interpreter::{Call, CallFrame, ExecuteError, Interpreter, LogEvent, MemoryRange};
    pub use crate::opcodes::Opcode;
    pub use crate::transaction::{Color, Id, Input, Output, Root, Transaction, ValidationError, Witness};
    pub use crate::types::*;
}
