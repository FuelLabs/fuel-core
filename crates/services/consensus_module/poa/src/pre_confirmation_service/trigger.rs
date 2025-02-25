use super::*;
use std::future::Future;
pub trait KeyRotationTrigger: Send {
    fn next_rotation(&self) -> impl Future<Output = Result<()>> + Send;
}
