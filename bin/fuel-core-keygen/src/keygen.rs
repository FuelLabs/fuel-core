use clap::Parser;
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
use serde_json::json;
use std::str::FromStr;

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
}

impl NewKey {
    fn exec(&self) -> anyhow::Result<()> {
        let mut rng = StdRng::from_entropy();
        let secret = SecretKey::random(&mut rng);
        let public_key = secret.public_key();
        let address = Input::owner(&public_key);
        let secret_str = secret.to_string();
        let output = json!({
            "secret": secret_str,
            "address": address,
        });
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
}

impl ParseSecret {
    fn exec(&self) -> anyhow::Result<()> {
        let secret = SecretKey::from_str(&self.secret)?;
        let address = Input::owner(&secret.public_key());
        let output = json!({
            "address": address.to_string(),
        });
        print_value(output, self.pretty)
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
