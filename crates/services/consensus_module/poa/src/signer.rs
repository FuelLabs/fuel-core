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
    fuel_crypto::PublicKey,
    fuel_tx::{
        Address,
        Input,
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
        cached_public_key_bytes: Vec<u8>,
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
            SignMode::Kms {
                key_id,
                client,
                cached_public_key_bytes,
            } => sign_with_kms(client, key_id, cached_public_key_bytes, message).await?,
        };
        Ok(Consensus::PoA(PoAConsensus::new(poa_signature)))
    }

    /// Returns the public key of the block producer, if any
    pub fn public_key(&self) -> anyhow::Result<Option<PublicKey>> {
        match self {
            SignMode::Unavailable => Ok(None),
            SignMode::Key(secret_key) => {
                Ok(Some(secret_key.expose_secret().public_key()))
            }

            #[cfg(feature = "aws-kms")]
            SignMode::Kms {
                cached_public_key_bytes,
                ..
            } => {
                use k256::pkcs8::DecodePublicKey;

                let k256_public_key =
                    k256::PublicKey::from_public_key_der(cached_public_key_bytes)?;
                Ok(Some(PublicKey::from(k256_public_key)))
            }
        }
    }

    /// Returns the address of the block producer, if any
    pub fn address(&self) -> anyhow::Result<Option<Address>> {
        let address = self.public_key()?.as_ref().map(Input::owner);
        Ok(address)
    }
}

#[cfg(feature = "aws-kms")]
async fn sign_with_kms(
    client: &aws_sdk_kms::Client,
    key_id: &str,
    public_key_bytes: &[u8],
    message: Message,
) -> anyhow::Result<Signature> {
    use k256::{
        ecdsa::{
            RecoveryId,
            VerifyingKey,
        },
        pkcs8::DecodePublicKey,
    };

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

    // This is a hack to get the recovery id. The signature should be normalized
    // before computing the recovery id, but aws kms doesn't support this, and
    // instead always computes the recovery id from non-normalized signature.
    // So instead the recovery id is determined by checking which variant matches
    // the original public key.

    let recid1 = RecoveryId::new(false, false);
    let recid2 = RecoveryId::new(true, false);

    let rec1 = VerifyingKey::recover_from_prehash(&*message, &sig, recid1);
    let rec2 = VerifyingKey::recover_from_prehash(&*message, &sig, recid2);

    let correct_public_key = k256::PublicKey::from_public_key_der(public_key_bytes)
        .map_err(|_| anyhow!("invalid DER public key from AWS KMS"))?
        .into();

    let recovery_id = if rec1.map(|r| r == correct_public_key).unwrap_or(false) {
        recid1
    } else if rec2.map(|r| r == correct_public_key).unwrap_or(false) {
        recid2
    } else {
        anyhow::bail!("Invalid signature generated (reduced-x form coordinate)");
    };

    // Insert the recovery id into the signature
    debug_assert!(
        !recovery_id.is_x_reduced(),
        "reduced-x form coordinates are caught by the if-else chain above"
    );
    let v = recovery_id.is_y_odd() as u8;
    let mut signature = <[u8; 64]>::from(sig.to_bytes());
    signature[32] = (v << 7) | (signature[32] & 0x7f);
    Ok(Signature::from_bytes(signature))
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

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
                cached_public_key_bytes: vec![], // Dummy value is ok for this test
            }
            .is_available())
        };
    }

    // The tests are against a keypair generated using fuel-core-keygen, which has
    // been tweaked to display the public key associated with a keypair.
    // The keypair used in these tests is the following:
    // {
    //    "address:"e2f3f5109c56eec359c124cbf25f35dcc1495b0bbdac5848e0ff37a86fe69a6d",
    //    "public_key":"9ab6229de634056cdc67dfba26e6a06e4ba082693ea30395e5994b113ab6c6e3189a12a10d8fb08d1d28f7117ca34f6b16c5132acd9570de6e7a005f6bbd8f3d",
    //    "secret":"2708b7bad8b5b52d031e5795c1d1995660185f464900cbd593328eb433bdb7f6","type":"block-production"
    // }
    #[test]
    fn public_key() {
        assert_eq!(SignMode::Unavailable.public_key().unwrap(), None);

        let secret_key = SecretKey::from_str(
            "2708b7bad8b5b52d031e5795c1d1995660185f464900cbd593328eb433bdb7f6",
        )
        .expect("Secret key construction should not fail");

        let public_key = "9ab6229de634056cdc67dfba26e6a06e4ba082693ea30395e5994b113ab6c6e3189a12a10d8fb08d1d28f7117ca34f6b16c5132acd9570de6e7a005f6bbd8f3d";

        let derived_public_key = SignMode::Key(Secret::new(secret_key.into()))
            .public_key()
            .expect("Public key derivation should not fail")
            .expect("Public key derivation should yield a defined value")
            .to_string();

        assert_eq!(public_key, &derived_public_key);
    }

    #[test]
    fn address() {
        assert_eq!(SignMode::Unavailable.address().unwrap(), None);

        let secret_key = SecretKey::from_str(
            "2708b7bad8b5b52d031e5795c1d1995660185f464900cbd593328eb433bdb7f6",
        )
        .expect("Secret key construction should not fail");

        let address = "e2f3f5109c56eec359c124cbf25f35dcc1495b0bbdac5848e0ff37a86fe69a6d";
        let derived_address = SignMode::Key(Secret::new(secret_key.into()))
            .address()
            .expect("Address derivation should not fail")
            .expect("Address derivation should yield a defined value")
            .to_string();

        assert_eq!(address, &derived_address);
    }
}
