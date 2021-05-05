#![feature(arbitrary_enum_discriminant)]

pub mod consts;
pub mod interpreter;
pub mod opcodes;
pub mod types;

pub mod prelude {
    pub use crate::interpreter::Interpreter;
    pub use crate::opcodes::Opcode;
    pub use crate::types::*;
}
