#![feature(arbitrary_enum_discriminant)]
#![feature(is_sorted)]

pub mod consts;
pub mod crypto;
pub mod interpreter;

pub mod prelude {
    pub use crate::interpreter::{Call, CallFrame, ExecuteError, Interpreter, LogEvent, MemoryRange};
    pub use fuel_asm::{Immediate06, Immediate12, Immediate18, Immediate24, Opcode, RegisterId, Word};
    pub use fuel_tx::{Color, Id, Input, Output, Root, Transaction, ValidationError, Witness};
}
