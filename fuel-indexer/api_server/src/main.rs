use anyhow::Result;
use async_std::{fs::File, io::ReadExt, net::SocketAddr};
use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use structopt::StructOpt;
use tracing_subscriber::filter::EnvFilter;

use api_server::GraphQlApi;


#[derive(Debug, Serialize, Deserialize)]
pub struct ServerConfig {
    /// API Server listen address.
    listen_address: SocketAddr,
    /// Where the data lives.
    database_url: String,
}


#[derive(Debug, StructOpt)]
#[structopt(
    name = "Indexer API Service",
    about = "Fuel indexer api"
)]
pub struct Args {
    #[structopt(short, long, help = "API Server config.")]
    config: PathBuf,
}

#[cfg(feature = "db-postgres")]
fn canonicalize(url: String) -> String {
    url
}

#[cfg(feature = "db-sqlite")]
fn canonicalize(url: String) -> String {
    let path = PathBuf::from(url).canonicalize().expect("Could not canonicalize path");
    path.into_os_string().into_string().expect("Could not stringify path")
}


#[tokio::main]
pub async fn main() -> Result<()> {
    let filter = match std::env::var_os("RUST_LOG") {
        Some(_) => EnvFilter::try_from_default_env().expect("Invalid `RUST_LOG` provided"),
        None => EnvFilter::new("info"),
    };

    tracing_subscriber::fmt::Subscriber::builder()
        .with_writer(std::io::stderr)
        .with_env_filter(filter)
        .init();

    let opt = Args::from_args();

    let mut file = File::open(opt.config).await?;
    let mut contents = String::new();
    file.read_to_string(&mut contents).await?;

    let ServerConfig {
        listen_address,
        database_url
    } = serde_yaml::from_str(&contents).expect("Bad yaml file");


    let api = GraphQlApi::new(canonicalize(database_url), listen_address);

    let api_handle = tokio::spawn(api.run());

    api_handle.await?;

    Ok(())
}
