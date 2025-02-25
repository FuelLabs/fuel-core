use super::*;
use crate::pre_confirmation_service::signing_key::SigningKey;

#[async_trait::async_trait]
pub trait KeyGenerator: Send {
    type Key: SigningKey;
    async fn generate(&mut self) -> Result<Self::Key>;
}
