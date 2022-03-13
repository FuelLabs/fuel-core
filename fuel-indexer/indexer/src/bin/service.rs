use anyhow::Result;
use async_std::{fs::File, io::ReadExt};
use fuel_core::service::{Config, FuelService};
use fuel_wasm_executor::{GraphQlApi, IndexerConfig, IndexerService, Manifest};
use std::path::PathBuf;
use structopt::StructOpt;
use tokio::join;
use tracing::error;
use tracing_subscriber::filter::EnvFilter;

#[derive(Debug, StructOpt)]
#[structopt(
    name = "Indexer Service",
    about = "Standalone binary for the fuel indexer service"
)]
pub struct Args {
    #[structopt(short, long, help = "run local test node")]
    local: bool,
    #[structopt(parse(from_os_str), help = "Indexer service config file")]
    config: PathBuf,
    #[structopt(short, long, parse(from_os_str), help = "Indexer service config file")]
    test_manifest: Option<PathBuf>,
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

    let mut config: IndexerConfig = serde_yaml::from_str(&contents)?;

    let _local_node = if opt.local {
        let s = FuelService::new_node(Config::local_node()).await.unwrap();
        config.fuel_node_addr = s.bound_address;
        Some(s)
    } else {
        None
    };

    let api_handle = tokio::spawn(GraphQlApi::run(config.clone()));

    let mut service = IndexerService::new(config)?;

    // TODO: need an API endpoint to upload/create these things.....
    if opt.test_manifest.is_some() {
        let mut path = opt.test_manifest.unwrap();

        let mut file = File::open(&path).await?;
        let mut contents = String::new();
        file.read_to_string(&mut contents).await?;
        let manifest: Manifest = serde_yaml::from_str(&contents)?;

        path.pop();
        path.push(&manifest.graphql_schema);
        let mut file = File::open(&path).await?;
        let mut schema = String::new();
        file.read_to_string(&mut schema).await?;

        path.pop();
        path.push(&manifest.wasm_module);
        let mut file = File::open(&path).await?;
        let mut bytes = Vec::<u8>::new();
        file.read_to_end(&mut bytes).await?;
        service.add_indexer(manifest, &schema, bytes, false)?;
    }

    let service_handle = tokio::spawn(service.run());

    let (first, second) = join!(api_handle, service_handle);

    if let Err(e) = first {
        error!("{:?}", e)
    }
    if let Err(e) = second {
        error!("{:?}", e)
    }
    Ok(())
}
