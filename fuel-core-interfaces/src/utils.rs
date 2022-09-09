pub mod signer {
    use crate::signer::{
        Signer,
        SignerError,
    };
    use fuel_types::Bytes32;

    pub struct DummySigner {}

    #[async_trait::async_trait]
    impl Signer for DummySigner {
        async fn sign(&self, hash: &Bytes32) -> Result<Bytes32, SignerError> {
            Ok(*hash)
        }
    }
}
