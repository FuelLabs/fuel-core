use core::fmt;

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
