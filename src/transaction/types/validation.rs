use std::{error, fmt, io};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ValidationError {
    InputCoinPredicateLength { index: usize },
    InputCoinPredicateDataLength { index: usize },
    InputCoinWitnessIndexBounds { index: usize },
    InputContractAssociatedOutputContract { index: usize },
    OutputContractInputIndex { index: usize },
    TransactionCreateInputContract { index: usize },
    TransactionCreateOutputContract { index: usize },
    TransactionCreateOutputVariable { index: usize },
    TransactionCreateOutputChangeColorZero { index: usize },
    TransactionCreateOutputChangeColorNonZero { index: usize },
    TransactionCreateOutputContractCreatedMultiple { index: usize },
    TransactionCreateBytecodeLen,
    TransactionCreateBytecodeWitnessIndex,
    TransactionCreateStaticContractsMax,
    TransactionCreateStaticContractsOrder,
    TransactionScriptLength,
    TransactionScriptDataLength,
    TransactionScriptOutputContractCreated { index: usize },
    TransactionGasLimit,
    TransactionMaturity,
    TransactionInputsMax,
    TransactionOutputsMax,
    TransactionWitnessesMax,
    TransactionOutputChangeColorDuplicated,
    TransactionOutputChangeColorNotFound,
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // TODO better describe the error variants
        write!(f, "{:?}", self)
    }
}

impl error::Error for ValidationError {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        None
    }
}

impl From<ValidationError> for io::Error {
    fn from(v: ValidationError) -> io::Error {
        io::Error::new(io::ErrorKind::Other, v)
    }
}
