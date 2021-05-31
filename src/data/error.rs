use std::{error, fmt};

#[derive(Debug)]
pub enum DataError {}

impl fmt::Display for DataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Execution error: {:?}", self)
    }
}

impl error::Error for DataError {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        None
    }
}
