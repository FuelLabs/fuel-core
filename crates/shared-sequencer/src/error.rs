use core::fmt;

use cosmrs::ErrorReport;

#[derive(Debug)]
pub struct CosmosError(pub ErrorReport);

impl fmt::Display for CosmosError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "CosmosError: {:?}", self.0)
    }
}

impl std::error::Error for CosmosError {}

impl From<ErrorReport> for CosmosError {
    fn from(e: ErrorReport) -> Self {
        Self(e)
    }
}

#[derive(Debug)]
pub struct PostBlobError {
    pub message: String,
}

impl fmt::Display for PostBlobError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PostBlobError: {:?}", self.message)
    }
}

impl std::error::Error for PostBlobError {}
