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
    fuel_crypto::Message,
    fuel_vm::Signature,
    secrecy::{
        ExposeSecret,
        Secret,
    },
};
use std::ops::Deref;

/// How the block is signed
#[derive(Clone, Debug)]
pub enum SignMode {
    /// Blocks cannot be produced
    Unavailable,
    /// Sign using a secret key
    Key(Secret<SecretKeyWrapper>),
    /// Sign using AWS KMS
    Kms {
        key_id: String,
        client: aws_sdk_kms::Client,
    },
}

impl SignMode {
    /// Is block sigining (production) available
    pub fn is_available(&self) -> bool {
        !matches!(self, SignMode::Unavailable)
    }

    /// Sign a block
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
            SignMode::Kms { key_id, client } => {
                sign_with_kms(client, key_id, message).await?
            }
        };
        Ok(Consensus::PoA(PoAConsensus::new(poa_signature)))
    }
}

async fn sign_with_kms(
    client: &aws_sdk_kms::Client,
    key_id: &str,
    message: Message,
) -> anyhow::Result<Signature> {
    let reply = client
        .sign()
        .key_id(key_id)
        .signing_algorithm(SigningAlgorithmSpec::EcdsaSha256)
        .message_type(MessageType::Digest)
        .message(Blob::new(*message))
        .send()
        .await
        .inspect_err(|err| tracing::error!("Failed to sign with AWS KMS: {err:?}"))?;

    let signature_der = reply
        .signature
        .ok_or_else(|| anyhow!("no signature returned from AWS KMS"))?
        .into_inner();
    // https://stackoverflow.com/a/71475108
    let sig = k256::ecdsa::Signature::from_der(&signature_der)
        .map_err(|_| anyhow!("invalid DER signature from AWS KMS"))?;
    let sig = sig.normalize_s().unwrap_or(sig);
    let sig_bytes = <[u8; 64]>::from(sig.to_bytes());
    Ok(Signature::from_bytes(sig_bytes))
}
