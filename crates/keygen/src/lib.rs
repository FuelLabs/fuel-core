#![deny(clippy::arithmetic_side_effects)]
#![deny(clippy::cast_possible_truncation)]

use clap::ValueEnum;
use fuel_core_types::{
    fuel_crypto::{
        rand::{
            prelude::StdRng,
            SeedableRng,
        },
        SecretKey,
    },
    fuel_tx::Input,
    fuel_types::Address,
};
use libp2p_identity::{
    secp256k1,
    Keypair,
    PeerId,
};
use serde::Serialize;
use std::{
    ops::Deref,
    str::FromStr,
};

#[derive(Clone, Copy, Debug, Default, Serialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum KeyType {
    #[default]
    BlockProduction,
    Peering,
}

impl From<KeyType> for &'static str {
    fn from(key_type: KeyType) -> Self {
        match key_type {
            KeyType::BlockProduction => "block-production",
            KeyType::Peering => "p2p",
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct ParseSecretResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    address: Option<Address>,
    #[serde(
        serialize_with = "serialize_option_to_string",
        skip_serializing_if = "Option::is_none"
    )]
    peer_id: Option<PeerId>,
    #[serde(rename = "type")]
    typ: KeyType,
}

#[derive(Clone, Debug, Serialize)]
pub struct NewKeyResponse {
    secret: SecretKey,
    #[serde(skip_serializing_if = "Option::is_none")]
    address: Option<Address>,
    #[serde(
        serialize_with = "serialize_option_to_string",
        skip_serializing_if = "Option::is_none"
    )]
    peer_id: Option<PeerId>,
    #[serde(rename = "type")]
    typ: KeyType,
}

fn serialize_option_to_string<S, T>(
    opt: &Option<T>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
    T: ToString,
{
    if let Some(value) = opt.as_ref() {
        value.to_string().serialize(serializer)
    } else {
        serializer.serialize_none()
    }
}

pub fn new_key(key_type: KeyType) -> anyhow::Result<NewKeyResponse> {
    let mut rng = StdRng::from_entropy();
    let secret = SecretKey::random(&mut rng);
    let public_key = secret.public_key();

    Ok(match key_type {
        KeyType::BlockProduction => {
            let address = Input::owner(&public_key);
            NewKeyResponse {
                secret,
                address: Some(address),
                peer_id: None,
                typ: key_type,
            }
        }
        KeyType::Peering => {
            let mut bytes = *secret.deref();
            let p2p_secret = secp256k1::SecretKey::try_from_bytes(&mut bytes)
                .expect("Should be a valid private key");
            let p2p_keypair = secp256k1::Keypair::from(p2p_secret);
            let libp2p_keypair = Keypair::from(p2p_keypair);
            let peer_id = PeerId::from_public_key(&libp2p_keypair.public());
            NewKeyResponse {
                secret,
                address: None,
                peer_id: Some(peer_id),
                typ: key_type,
            }
        }
    })
}

pub fn parse_secret(
    key_type: KeyType,
    secret: &str,
) -> anyhow::Result<ParseSecretResponse> {
    let secret =
        SecretKey::from_str(secret).map_err(|_| anyhow::anyhow!("invalid secret key"))?;
    Ok(match key_type {
        KeyType::BlockProduction => {
            let address = Input::owner(&secret.public_key());
            ParseSecretResponse {
                address: Some(address),
                peer_id: None,
                typ: key_type,
            }
        }
        KeyType::Peering => {
            let mut bytes = *secret.deref();
            let p2p_secret = secp256k1::SecretKey::try_from_bytes(&mut bytes)
                .expect("Should be a valid private key");
            let p2p_keypair = secp256k1::Keypair::from(p2p_secret);
            let libp2p_keypair = Keypair::from(p2p_keypair);
            let peer_id = PeerId::from_public_key(&libp2p_keypair.public());
            ParseSecretResponse {
                address: None,
                peer_id: Some(peer_id),
                typ: key_type,
            }
        }
    })
}
