use anyhow::anyhow;
#[cfg(feature = "aws-kms")]
use aws_sdk_kms::{
    primitives::Blob,
    types::{
        MessageType,
        SigningAlgorithmSpec,
    },
};
#[cfg(feature = "aws-kms")]
use fuel_core_types::fuel_crypto::Message;
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

/// How the block is signed
#[derive(Clone, Debug)]
pub enum SignMode {
    /// Blocks cannot be produced
    Unavailable,
    /// Sign using a secret key
    Key(Secret<SecretKeyWrapper>),
    /// Sign using AWS KMS
    #[cfg(feature = "aws-kms")]
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

        let poa_signature = match self {
            SignMode::Unavailable => return Err(anyhow!("no PoA signing key configured")),
            SignMode::Key(key) => {
                let signing_key = key.expose_secret().deref();
                Signature::sign(signing_key, &message)
            }
            #[cfg(feature = "aws-kms")]
            SignMode::Kms { key_id, client } => {
                sign_with_kms(client, key_id, message).await?
            }
        };
        Ok(Consensus::PoA(PoAConsensus::new(poa_signature)))
    }
}

#[cfg(feature = "aws-kms")]
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

#[cfg(test)]
mod tests {
    use super::*;
    use fuel_core_types::fuel_crypto::SecretKey;
    use rand::{
        rngs::StdRng,
        SeedableRng,
    };

    #[cfg(not(feature = "aws-kms"))]
    use aws_config as _;

    #[tokio::test]
    async fn sign_mode_is_available() {
        let mut rng = StdRng::seed_from_u64(2322);
        let secret_key = SecretKey::random(&mut rng);
        assert!(!SignMode::Unavailable.is_available());
        let signer = SignMode::Key(Secret::new(secret_key.into()));
        assert!(signer.is_available());
        #[cfg(feature = "aws-kms")]
        {
            // This part of the test is only enabled if the environment variable is set
            let Some(kms_arn) = option_env!("FUEL_CORE_TEST_AWS_KMS_ARN") else {
                return;
            };
            let config = aws_config::load_from_env().await;
            let kms_client = aws_sdk_kms::Client::new(&config);
            assert!(SignMode::Kms {
                key_id: kms_arn.to_string(),
                client: kms_client,
            }
            .is_available())
        };
    }
}
