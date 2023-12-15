use std::{collections::BTreeMap, sync::Arc};

use graph::{
    anyhow::{bail, Context},
    components::subgraph::{Setting, Settings},
    endpoint::EndpointMetrics,
    env::EnvVars,
    itertools::Itertools,
    prelude::{
        anyhow::{anyhow, Error},
        MetricsRegistry, NodeId, SubgraphName,
    },
    slog::Logger,
};
use graph_chain_ethereum::{NodeCapabilities, ProviderEthRpcMetrics};
use graph_store_postgres::DeploymentPlacer;

use crate::{chain::create_ethereum_networks_for_chain, config::Config};

pub fn place(placer: &dyn DeploymentPlacer, name: &str, network: &str) -> Result<(), Error> {
    match placer.place(name, network).map_err(|s| anyhow!(s))? {
        None => {
            println!(
                "no matching placement rule; default placement from JSON RPC call would be used"
            );
        }
        Some((shards, nodes)) => {
            let nodes: Vec<_> = nodes.into_iter().map(|n| n.to_string()).collect();
            let shards: Vec<_> = shards.into_iter().map(|s| s.to_string()).collect();
            println!("subgraph: {}", name);
            println!("network:  {}", network);
            println!("shard:    {}", shards.join(", "));
            println!("nodes:    {}", nodes.join(", "));
        }
    }
    Ok(())
}

pub fn check(config: &Config, print: bool) -> Result<(), Error> {
    match config.to_json() {
        Ok(txt) => {
            if print {
                println!("{}", txt);
                return Ok(());
            }
        }
        Err(e) => bail!("error serializing config: {}", e),
    }

    let env_vars = EnvVars::from_env().unwrap();
    if let Some(path) = &env_vars.subgraph_settings {
        match Settings::from_file(path) {
            Ok(_) => {
                println!("Successfully validated subgraph settings from {path}");
            }
            Err(e) => {
                eprintln!("configuration error in subgraph settings {}: {}", path, e);
                std::process::exit(1);
            }
        }
    };

    println!("Successfully validated configuration");
    Ok(())
}

pub fn pools(config: &Config, nodes: Vec<String>, shard: bool) -> Result<(), Error> {
    // Quietly replace `-` with `_` in node names to make passing in pod names
    // from k8s less annoying
    let nodes: Vec<_> = nodes
        .into_iter()
        .map(|name| {
            NodeId::new(name.replace('-', "_"))
                .map_err(|()| anyhow!("illegal node name `{}`", name))
        })
        .collect::<Result<_, _>>()?;
    // node -> shard_name -> size
    let mut sizes = BTreeMap::new();
    for node in &nodes {
        let mut shard_sizes = BTreeMap::new();
        for (name, shard) in &config.stores {
            let size = shard.pool_size.size_for(node, name)?;
            shard_sizes.insert(name.to_string(), size);
            for (replica_name, replica) in &shard.replicas {
                let qname = format!("{}.{}", name, replica_name);
                let size = replica.pool_size.size_for(node, &qname)?;
                shard_sizes.insert(qname, size);
            }
        }
        sizes.insert(node.to_string(), shard_sizes);
    }

    if shard {
        let mut by_shard: BTreeMap<&str, u32> = BTreeMap::new();
        for shard_sizes in sizes.values() {
            for (shard_name, size) in shard_sizes {
                *by_shard.entry(shard_name).or_default() += size;
            }
        }
        for (shard_name, size) in by_shard {
            println!("{}: {}", shard_name, size);
        }
    } else {
        for node in &nodes {
            let empty = BTreeMap::new();
            println!("{}:", node);
            let node_sizes = sizes.get(node.as_str()).unwrap_or(&empty);
            for (shard, size) in node_sizes {
                println!("    {}: {}", shard, size);
            }
        }
    }
    Ok(())
}

pub async fn provider(
    logger: Logger,
    config: &Config,
    registry: Arc<MetricsRegistry>,
    features: String,
    network: String,
) -> Result<(), Error> {
    // Like NodeCapabilities::from_str but with error checking for typos etc.
    fn caps_from_features(features: String) -> Result<NodeCapabilities, Error> {
        let mut caps = NodeCapabilities {
            archive: false,
            traces: false,
        };
        for feature in features.split(',') {
            match feature {
                "archive" => caps.archive = true,
                "traces" => caps.traces = true,
                _ => bail!("unknown feature {}", feature),
            }
        }
        Ok(caps)
    }

    let metrics = Arc::new(EndpointMetrics::mock());
    let caps = caps_from_features(features)?;
    let eth_rpc_metrics = Arc::new(ProviderEthRpcMetrics::new(registry));
    let networks =
        create_ethereum_networks_for_chain(&logger, eth_rpc_metrics, config, &network, metrics)
            .await?;
    let adapters = networks
        .networks
        .get(&network)
        .ok_or_else(|| anyhow!("unknown network {}", network))?;
    let adapters = adapters.all_cheapest_with(&caps);
    println!(
        "deploy on network {} with features [{}] on node {}\neligible providers: {}",
        network,
        caps,
        config.node.as_str(),
        adapters
            .map(|adapter| adapter.provider().to_string())
            .join(", ")
    );
    Ok(())
}

pub fn setting(name: &str) -> Result<(), Error> {
    let name = SubgraphName::new(name).map_err(|()| anyhow!("illegal subgraph name `{}`", name))?;
    let env_vars = EnvVars::from_env().unwrap();
    if let Some(path) = &env_vars.subgraph_settings {
        let settings = Settings::from_file(path)
            .with_context(|| format!("syntax error in subgraph settings `{}`", path))?;
        match settings.for_name(&name) {
            Some(Setting { history_blocks, .. }) => {
                println!("setting for `{name}` will use history_blocks = {history_blocks}");
            }
            None => {
                println!("no specific setting for `{name}`, defaults will be used");
            }
        }
    } else {
        println!("No subgraph-specific settings will be applied because");
        println!("GRAPH_EXPERIMENTAL_SUBGRAPH_SETTINGS is not set");
    };

    Ok(())
}
