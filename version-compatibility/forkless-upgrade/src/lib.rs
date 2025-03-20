#![deny(unused_crate_dependencies)]
#![deny(warnings)]

#[cfg(test)]
use fuel_core::{
    chain_config::{
        SnapshotMetadata,
        SnapshotReader,
    },
    p2p_test_helpers::Bootstrap,
    service::Config,
};

#[cfg(test)]
use async_graphql as _;

#[cfg(test)]
mod backward_compatibility;
#[cfg(test)]
mod forward_compatibility;

#[cfg(test)]
mod gas_price_algo_compatibility;

#[cfg(test)]
mod genesis;
#[cfg(test)]
pub(crate) mod tests_helper;

#[cfg(test)]
fuel_core_trace::enable_tracing!();

#[cfg(test)]
pub async fn bootstrap_node(path: &str) -> anyhow::Result<(Bootstrap, String)> {
    let metadata = SnapshotMetadata::read(path)?;
    let reader = SnapshotReader::open(metadata)?;
    let config = Config::local_node_with_reader(reader);

    let node = Bootstrap::new(&config).await?;

    let addresses = node.listeners();
    let addresses = addresses
        .into_iter()
        .map(|addr| addr.to_string())
        .collect::<Vec<_>>();
    let cli_argument = addresses.join(",");

    Ok((node, cli_argument))
}
