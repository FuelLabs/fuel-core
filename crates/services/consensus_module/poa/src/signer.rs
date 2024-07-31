use anyhow::anyhow;
use aws_sdk_kms::{
    primitives::Blob,
    types::{
        MessageType,
        SigningAlgorithmSpec,
    },
};
use fuel_core_types::{
    blockchain::{
        block::Block,
        consensus::{
            poa::PoAConsensus,
            Consensus,
        },
        primitives::SecretKeyWrapper,
    },
    fuel_vm::Signature,
    secrecy::{
        ExposeSecret,
        Secret,
    },
};
use std::ops::Deref;

#[derive(Clone, Debug)]
pub enum SignMode {
    /// Blocks cannot be produced
    Unavailable,
    /// Sign using a secret key
    Key(Secret<SecretKeyWrapper>),
    /// Sign using AWS KMS
    Kms(aws_sdk_kms::Client),
}

impl SignMode {
    pub fn is_available(&self) -> bool {
        !matches!(self, SignMode::Unavailable)
    }

    pub async fn seal_block(&self, block: &Block) -> anyhow::Result<Consensus> {
        let block_hash = block.id();
        let message = block_hash.into_message();

        // https://docs.aws.amazon.com/kms/latest/developerguide/asymmetric-key-specs.html
        // const AWS_KEY_TYPE: KeySpec = KeySpec::ECC_SECG_P256K1;

        let poa_signature = match self {
            SignMode::Unavailable => return Err(anyhow!("no PoA signing key configured")),
            SignMode::Key(key) => {
                // The length of the secret is checked
                let signing_key = key.expose_secret().deref();
                Signature::sign(signing_key, &message)
            }
            SignMode::Kms(client) => {
                let reply = client
                    .sign()
                    // .key_id(key_id)
                    .signing_algorithm(SigningAlgorithmSpec::EcdsaSha256)
                    .message_type(MessageType::Digest)
                    .message(Blob::new(*message))
                    .send()
                    .await?;
                let signature = reply
                    .signature
                    .ok_or_else(|| anyhow!("no signature returned from AWS KMS"))?
                    .into_inner();
                assert_eq!(
                    signature.len(),
                    64,
                    "incorrect signature length from AWS KMS"
                );
                let mut signature_bytes = [0u8; 64];
                signature_bytes.copy_from_slice(&signature);
                Signature::from_bytes(signature_bytes)
            }
        };
        Ok(Consensus::PoA(PoAConsensus::new(poa_signature)))
    }
}

// async fn foo() {
//     let config = aws_config::load_from_env().await;
//     let client = aws_sdk_kms::Client::new(&config);
// }
