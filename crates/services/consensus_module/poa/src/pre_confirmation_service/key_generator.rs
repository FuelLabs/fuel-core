use super::*;
use crate::pre_confirmation_service::signing_key::SigningKey;
use std::future::Future;

pub trait KeyGenerator: Send {
    type Key: SigningKey;
    fn generate(&mut self) -> impl Future<Output = Result<Self::Key>> + Send;
}
