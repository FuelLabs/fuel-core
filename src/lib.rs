#![feature(arbitrary_enum_discriminant)]

pub mod consts;
pub mod crypto;
pub mod interpreter;
pub mod opcodes;
pub mod transaction;
pub mod types;

pub mod prelude {
    pub use crate::interpreter::{ExecuteError, Interpreter};
    pub use crate::opcodes::Opcode;
    pub use crate::transaction::{Color, Id, Input, Output, Root, Transaction, Witness};
    pub use crate::types::*;
}
