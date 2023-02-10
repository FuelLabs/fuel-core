use clap::{
    Parser,
    ValueEnum,
};
use fuel_core_types::{
    fuel_crypto::{
        rand::{
            prelude::StdRng,
            SeedableRng,
        },
        SecretKey,
    },
    fuel_tx::Input,
};
use libp2p_core::{
    identity::{
        secp256k1,
        Keypair,
    },
    PeerId,
};
use serde_json::json;
use std::{
    ops::Deref,
    str::FromStr,
};

/// Key management utilities for configuring fuel-core
#[derive(Debug, Parser)]
pub(crate) enum Command {
    New(NewKey),
    Parse(ParseSecret),
}

impl Command {
    pub(crate) fn exec(&self) -> anyhow::Result<()> {
        match self {
            Command::New(cmd) => cmd.exec(),
            Command::Parse(cmd) => cmd.exec(),
        }
    }
}

/// Generate a random new secret & public key in the format expected by fuel-core
#[derive(Debug, clap::Args)]
#[clap(author, version, about)]
pub(crate) struct NewKey {
    #[clap(long = "pretty", short = 'p')]
    pretty: bool,
    #[clap(
        long = "key-type",
        short = 'k',
        value_enum,
        default_value = "block-production"
    )]
    key_type: KeyType,
}

#[derive(Clone, Debug, Default, ValueEnum)]
pub(crate) enum KeyType {
    #[default]
    BlockProduction,
    Peering,
}

impl NewKey {
    fn exec(&self) -> anyhow::Result<()> {
        let mut rng = StdRng::from_entropy();
        let secret = SecretKey::random(&mut rng);
        let public_key = secret.public_key();
        let secret_str = secret.to_string();

        let output = match self.key_type {
            KeyType::BlockProduction => {
                let address = Input::owner(&public_key);
                json!({
                    "secret": secret_str,
                    "address": address,
                    "type": "block_production"
                })
            }
            KeyType::Peering => {
                let mut bytes = *secret.deref();
                let p2p_secret = secp256k1::SecretKey::from_bytes(&mut bytes)
                    .expect("Should be a valid private key");
                let libp2p_keypair = Keypair::Secp256k1(p2p_secret.into());
                let peer_id = PeerId::from_public_key(&libp2p_keypair.public());
                json!({
                    "secret": secret_str,
                    "peer_id": peer_id.to_string(),
                    "type": "p2p"
                })
            }
        };
        print_value(output, self.pretty)
    }
}

/// Parse a secret key to view the associated public key
#[derive(Debug, clap::Args)]
#[clap(author, version, about)]
pub(crate) struct ParseSecret {
    secret: String,
    #[clap(long = "pretty", short = 'p')]
    pretty: bool,
    #[clap(
        long = "key-type",
        short = 'k',
        value_enum,
        default_value = "block-production"
    )]
    key_type: KeyType,
}

impl ParseSecret {
    fn exec(&self) -> anyhow::Result<()> {
        let secret = SecretKey::from_str(&self.secret)?;
        match self.key_type {
            KeyType::BlockProduction => {
                let address = Input::owner(&secret.public_key());
                let output = json!({
                    "address": address.to_string(),
                    "type": "block-production",
                });
                print_value(output, self.pretty)
            }
            KeyType::Peering => {
                let mut bytes = *secret.deref();
                let p2p_secret = secp256k1::SecretKey::from_bytes(&mut bytes)
                    .expect("Should be a valid private key");
                let libp2p_keypair = Keypair::Secp256k1(p2p_secret.into());
                let peer_id = PeerId::from_public_key(&libp2p_keypair.public());
                let output = json!({
                    "peer_id": peer_id.to_string(),
                    "type": "p2p"
                });
                print_value(output, self.pretty)
            }
        }
    }
}

fn print_value(output: serde_json::Value, pretty: bool) -> anyhow::Result<()> {
    let output = if pretty {
        serde_json::to_string_pretty(&output)
    } else {
        serde_json::to_string(&output)
    };
    println!("{}", output?);
    Ok(())
}
