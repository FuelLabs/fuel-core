//! A simple keygen cli utility tool for configuring fuel-core
use clap::Parser;
use crossterm::terminal;
use fuel_core_keygen::{
    new_key,
    parse_secret,
    KeyType,
};
use std::io::{
    stdin, stdout, IsTerminal, Read, Write
};
use termion::screen::IntoAlternateScreen;

/// Parse a secret key to view the associated public key
#[derive(Debug, clap::Args)]
pub struct ParseSecret {
    /// A private key in hex format
    secret: String,
    /// Print the JSON in pretty format
    #[clap(long = "pretty", short = 'p')]
    pub pretty: bool,
    /// Key type to generate. It can either be `block-production` or `peering`.
    #[clap(
        long = "key-type",
        short = 'k',
        value_enum,
        default_value = <KeyType as std::convert::Into<&'static str>>::into(KeyType::BlockProduction),
    )]
    pub key_type: KeyType,
}

/// Generate a random new secret & public key in the format expected by fuel-core
#[derive(Debug, clap::Args)]
pub struct NewKey {
    /// Print the JSON in pretty format
    #[clap(long = "pretty", short = 'p')]
    pub pretty: bool,
    /// Key type to generate. It can either be `block-production` or `peering`.
    #[clap(
        long = "key-type",
        short = 'k',
        value_enum,
        default_value = <KeyType as std::convert::Into<&'static str>>::into(KeyType::BlockProduction),
    )]
    pub key_type: KeyType,
}

/// Key management utilities for configuring fuel-core
#[derive(Debug, Parser)]
#[clap(name = "fuel-core-keygen", author, version, about)]
pub(crate) enum Command {
    New(NewKey),
    Parse(ParseSecret),
}

impl Command {
    pub(crate) fn exec(&self) -> anyhow::Result<(serde_json::Value, bool)> {
        match self {
            Command::New(cmd) => {
                Ok((serde_json::to_value(new_key(cmd.key_type)?)?, cmd.pretty))
            }
            Command::Parse(cmd) => Ok((
                serde_json::to_value(parse_secret(cmd.key_type, &cmd.secret)?)?,
                cmd.pretty,
            )),
        }
    }
}

fn main() -> anyhow::Result<()> {
    let cmd = Command::parse();
    let (result, is_pretty) = cmd.exec()?;
    print_value(result, is_pretty)
}

fn wait_for_keypress() {
    let mut single_key = [0u8];
    terminal::enable_raw_mode().expect("enable_raw_mode failed");
    stdin().read_exact(&mut single_key).unwrap();
    terminal::disable_raw_mode().expect("disable_raw_mode failed");
}

fn display_string_discreetly(
    discreet_string: &str,
    continue_message: &str,
) -> anyhow::Result<()> {
    if stdout().is_terminal() {
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
