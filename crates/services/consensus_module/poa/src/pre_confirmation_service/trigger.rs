use super::*;
#[async_trait::async_trait]
pub trait KeyRotationTrigger: Send {
    async fn next_rotation(&self) -> Result<()>;
}
