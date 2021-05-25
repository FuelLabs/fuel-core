use fuel_asm::Opcode;
use fuel_tx::ValidationError;

use std::{error, fmt};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ExecuteError {
    OpcodeFailure(Opcode),
    ValidationError(ValidationError),
    TransactionCreateStaticContractNotFound,
    TransactionCreateIdNotInTx,
    StackOverflow,
    PredicateOverflow,
    PredicateFailure,
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

            _ => write!(f, "Execution error: {:?}", self),
        }
    }
}

impl error::Error for ExecuteError {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match self {
            Self::ValidationError(e) => Some(e),
            _ => None,
        }
    }
}

impl From<ValidationError> for ExecuteError {
    fn from(e: ValidationError) -> Self {
        Self::ValidationError(e)
    }
}
