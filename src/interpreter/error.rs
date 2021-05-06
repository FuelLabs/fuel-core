use crate::opcodes::Opcode;

use std::{error, fmt};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExecuteError {
    OpcodeFailure(Opcode),
}

impl fmt::Display for ExecuteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OpcodeFailure(op) => {
                write!(f, "Failed to execute the opcode: {:?}", op)
            }
        }
    }
}

impl error::Error for ExecuteError {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        None
    }
}
