use fuel_asm::Opcode;
use fuel_tx::ValidationError;

use std::{error, fmt, io};

#[derive(Debug)]
pub enum ExecuteError {
    OpcodeFailure(Opcode),
    ValidationError(ValidationError),
    Io(io::Error),
    TransactionCreateStaticContractNotFound,
    TransactionCreateIdNotInTx,
    StackOverflow,
    PredicateOverflow,
    ProgramOverflow,
    PredicateFailure,
    ContractNotFound,
}

impl fmt::Display for ExecuteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OpcodeFailure(op) => {
                write!(f, "Failed to execute the opcode: {:?}", op)
            }

            Self::ValidationError(e) => {
                write!(f, "Failed to validate the transaction: {}", e)
            }

            Self::Io(e) => {
                write!(f, "I/O failure: {}", e)
            }

            _ => write!(f, "Execution error: {:?}", self),
        }
    }
}

impl error::Error for ExecuteError {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match self {
            Self::ValidationError(e) => Some(e),
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<ValidationError> for ExecuteError {
    fn from(e: ValidationError) -> Self {
        Self::ValidationError(e)
    }
}

impl From<io::Error> for ExecuteError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}
