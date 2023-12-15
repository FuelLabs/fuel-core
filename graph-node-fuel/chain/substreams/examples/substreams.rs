use anyhow::{format_err, Context, Error};
use graph::blockchain::block_stream::BlockStreamEvent;
use graph::blockchain::client::ChainClient;
use graph::blockchain::substreams_block_stream::SubstreamsBlockStream;
use graph::endpoint::EndpointMetrics;
use graph::firehose::{FirehoseEndpoints, SubgraphLimit};
use graph::prelude::{info, tokio, DeploymentHash, MetricsRegistry, Registry};
use graph::tokio_stream::StreamExt;
use graph::{env::env_var, firehose::FirehoseEndpoint, log::logger, substreams};
use graph_chain_substreams::mapper::Mapper;
use prost::Message;
use std::env;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Error> {
    let module_name = env::args().nth(1).unwrap();

    let token_env = env_var("SUBSTREAMS_API_TOKEN", "".to_string());
    let mut token: Option<String> = None;
    if !token_env.is_empty() {
        token = Some(token_env);
    }

    let endpoint = env_var(
        "SUBSTREAMS_ENDPOINT",
        "https://api.streamingfast.io".to_string(),
    );

    let package_file = env_var("SUBSTREAMS_PACKAGE", "".to_string());
    if package_file.is_empty() {
        panic!("Environment variable SUBSTREAMS_PACKAGE must be set");
    }

    let package = read_package(&package_file)?;

    let logger = logger(true);
    // Set up Prometheus registry
    let prometheus_registry = Arc::new(Registry::new());
    let metrics_registry = Arc::new(MetricsRegistry::new(
        logger.clone(),
        prometheus_registry.clone(),
    ));

    let endpoint_metrics = EndpointMetrics::new(
        logger.clone(),
        &[endpoint.clone()],
        Arc::new(MetricsRegistry::mock()),
    );

    let firehose = Arc::new(FirehoseEndpoint::new(
        "substreams",
        &endpoint,
        token,
        false,
        false,
        SubgraphLimit::Unlimited,
        Arc::new(endpoint_metrics),
    ));

    let client = Arc::new(ChainClient::new_firehose(FirehoseEndpoints::from(vec![
        firehose,
    ])));

    let mut stream: SubstreamsBlockStream<graph_chain_substreams::Chain> =
        SubstreamsBlockStream::new(
            DeploymentHash::new("substreams".to_string()).unwrap(),
            client,
            None,
            None,
            Arc::new(Mapper {
                schema: None,
                skip_empty_blocks: false,
            }),
            package.modules.clone(),
            module_name.to_string(),
            vec![12369621],
            vec![],
            logger.clone(),
            metrics_registry,
        );

    loop {
        match stream.next().await {
            None => {
                break;
            }
            Some(event) => match event {
                Err(_) => {}
                Ok(block_stream_event) => match block_stream_event {
                    BlockStreamEvent::ProcessWasmBlock(_, _, _, _) => {
                        unreachable!("Cannot happen with this mapper")
                    }
                    BlockStreamEvent::Revert(_, _) => {}
                    BlockStreamEvent::ProcessBlock(block_with_trigger, _) => {
                        for change in block_with_trigger.block.changes.entity_changes {
                            for field in change.fields {
                                info!(&logger, "field: {:?}", field);
                            }
                        }
                    }
                },
            },
        }
    }

    Ok(())
}

fn read_package(file: &str) -> Result<substreams::Package, anyhow::Error> {
    let content = std::fs::read(file).context(format_err!("read package {}", file))?;
    substreams::Package::decode(content.as_ref()).context("decode command")
}
