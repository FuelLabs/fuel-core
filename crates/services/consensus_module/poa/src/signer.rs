use anyhow::{
    anyhow,
    Context,
};
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
                let reply = client
                    .sign()
                    .key_id(key_id)
                    .signing_algorithm(SigningAlgorithmSpec::EcdsaSha256)
                    .message_type(MessageType::Digest)
                    .message(Blob::new(*message))
                    .send()
                    .await?;
                let signature_der = reply
                    .signature
                    .ok_or_else(|| anyhow!("no signature returned from AWS KMS"))?
                    .into_inner();
                // https://stackoverflow.com/a/71475108
                parse_der_signature(&signature_der)?
            }
        };
        Ok(Consensus::PoA(PoAConsensus::new(poa_signature)))
    }
}

/// https://datatracker.ietf.org/doc/html/rfc3279#section-2.2.3
fn parse_der_signature(signature_der: &[u8]) -> anyhow::Result<Signature> {
    let (rest, obj) = der_parser::der::parse_der(signature_der)
        .context("Parsing DER signature from AWS KMS")?;

    if !rest.is_empty() {
        anyhow::bail!("Extra bytes after DER signature: {:?}", rest);
    }

    let der_parser::ber::BerObjectContent::Sequence(mut seq) = obj.content else {
        anyhow::bail!("Expected a sequence of (r, s) in DER signature");
    };

    if seq.len() != 2 {
        anyhow::bail!(
            "Invalid signature sequence length (expected (r, s)): {}",
            seq.len()
        );
    }

    let s = parse_der_integer_32bytes(seq.pop().unwrap()).context("s")?;
    let r = parse_der_integer_32bytes(seq.pop().unwrap()).context("r")?;

    let mut signature_bytes = [0u8; 64];
    signature_bytes[..32].copy_from_slice(&r);
    signature_bytes[32..].copy_from_slice(&s);
    Ok(Signature::from_bytes(signature_bytes))
}

fn parse_der_integer_32bytes(
    obj: der_parser::ber::BerObject,
) -> anyhow::Result<[u8; 32]> {
    let der_parser::ber::BerObjectContent::Integer(value) = obj.content else {
        anyhow::bail!("Expected an integer for in DER signature");
    };

    if value.len() != 33 {
        anyhow::bail!(
            "Invalid signature element value length: {} (expected 33)",
            value.len()
        );
    }

    if value[0] == 0x02 {
        anyhow::bail!("Expected integer 0x02 prefix in DER signature");
    }

    let mut bytes = [0u8; 32];
    bytes.copy_from_slice(&value[1..]);
    Ok(bytes)
}
