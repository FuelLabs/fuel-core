use anyhow::Result;
use async_std::{fs::File, io::ReadExt, net::SocketAddr};
use fuel_core::service::{Config, FuelService};
use fuel_indexer_schema::graphql::Schema;
use std::path::PathBuf;
use async_process::Command;
use structopt::StructOpt;
use tokio::join;
use tracing::error;
use tracing_subscriber::filter::EnvFilter;

//use crate::GraphQlApi;


#[derive(Debug, StructOpt)]
#[structopt(
    name = "Indexer Service",
    about = "Standalone binary for the fuel indexer service"
)]
pub struct Args {
    #[structopt(short, long, help = "run local test node")]
    local: bool,
    #[structopt(short, long, conflicts_with = "local", help = "node address")]
    node_address: Option<SocketAddr>,
    #[structopt(parse(from_os_str), help = "Indexer service config file")]
    config: PathBuf,
    #[structopt(short, long, parse(from_os_str), help = "Indexer service config file")]
    manifest: PathBuf,
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
    let fuel_node_addr;

    let _local_node = if opt.local {
        let s = FuelService::new_node(Config::local_node()).await.unwrap();
        fuel_node_addr = s.bound_address;
        Some(s)
    } else {
        fuel_node_addr = opt.node_address.unwrap();
        None
    };

    let mut child = Command::new("indexer_service")
        .args(&[&opt.config.into_os_string().into_string().unwrap(), "--manifest", &opt.manifest.into_os_string().into_string().unwrap()])
        .spawn()
        .expect("Failed to start");

    let mut output = String::new();
    let mut stderr = child.stderr.take().unwrap();
    stderr.read_to_string(&mut output);

    let schema: Schema = serde_json::from_str(&output).expect("Bad schema");
    println!("service {:?}", schema);

    // TODO: new config fmt...
    //let api_handle = tokio::spawn(GraphQlApi::run(config.clone()));

    //api_handle.await?;

    Ok(())
}
