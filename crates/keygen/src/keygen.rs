use crate::{
    BLOCK_PRODUCTION,
    P2P,
};
use atty::Stream;
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
};
use libp2p_identity::{
    secp256k1,
    Keypair,
    PeerId,
};
use serde_json::json;
use std::{
    io::{
        stdin,
        stdout,
        Read,
        Write,
    },
    ops::Deref,
    str::FromStr,
};
use termion::screen::IntoAlternateScreen;

/// Generate a random new secret & public key in the format expected by fuel-core
#[derive(Debug, clap::Args)]
#[clap(author, version, about)]
pub struct NewKey {
    #[clap(long = "pretty", short = 'p')]
    pretty: bool,
    #[clap(
        long = "key-type",
        short = 'k',
        value_enum,
        default_value = BLOCK_PRODUCTION,
    )]
    key_type: KeyType,
}

#[derive(Clone, Debug, Default, ValueEnum)]
pub enum KeyType {
    #[default]
    BlockProduction,
    Peering,
}

impl NewKey {
    pub fn exec(&self) -> anyhow::Result<()> {
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
                    "type": BLOCK_PRODUCTION,
                })
            }
            KeyType::Peering => {
                let mut bytes = *secret.deref();
                let p2p_secret = secp256k1::SecretKey::try_from_bytes(&mut bytes)
                    .expect("Should be a valid private key");
                let p2p_keypair = secp256k1::Keypair::from(p2p_secret);
                let libp2p_keypair = Keypair::from(p2p_keypair);
                let peer_id = PeerId::from_public_key(&libp2p_keypair.public());
                json!({
                    "secret": secret_str,
                    "peer_id": peer_id.to_string(),
                    "type": P2P
                })
            }
        };
        print_value(output, self.pretty)
    }
}

/// Parse a secret key to view the associated public key
#[derive(Debug, clap::Args)]
#[clap(author, version, about)]
pub struct ParseSecret {
    secret: String,
    #[clap(long = "pretty", short = 'p')]
    pretty: bool,
    #[clap(
        long = "key-type",
        short = 'k',
        value_enum,
        default_value = BLOCK_PRODUCTION,
    )]
    key_type: KeyType,
}

impl ParseSecret {
    pub fn exec(&self) -> anyhow::Result<()> {
        let secret = SecretKey::from_str(&self.secret)
            .map_err(|_| anyhow::anyhow!("invalid secret key"))?;
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
                let p2p_secret = secp256k1::SecretKey::try_from_bytes(&mut bytes)
                    .expect("Should be a valid private key");
                let p2p_keypair = secp256k1::Keypair::from(p2p_secret);
                let libp2p_keypair = Keypair::from(p2p_keypair);
                let peer_id = PeerId::from_public_key(&libp2p_keypair.public());
                let output = json!({
                    "peer_id": peer_id.to_string(),
                    "type": P2P
                });
                print_value(output, self.pretty)
            }
        }
    }
}

fn wait_for_keypress() {
    let mut single_key = [0u8];
    stdin().read_exact(&mut single_key).unwrap();
}

fn display_string_discreetly(
    discreet_string: &str,
    continue_message: &str,
) -> anyhow::Result<()> {
    if atty::is(Stream::Stdout) {
        let mut screen = stdout().into_alternate_screen()?;
        writeln!(screen, "{discreet_string}")?;
        screen.flush()?;
        println!("{continue_message}");
        wait_for_keypress();
    } else {
        println!("{discreet_string}");
    }
    Ok(())
}

fn print_value(output: serde_json::Value, pretty: bool) -> anyhow::Result<()> {
    let output = if pretty {
        serde_json::to_string_pretty(&output)
    } else {
        serde_json::to_string(&output)
    }
    .map_err(anyhow::Error::msg);

    let _ = display_string_discreetly(
        &output?,
        "### Do not share or lose this private key! Press any key to complete. ###",
    );
    Ok(())
}
