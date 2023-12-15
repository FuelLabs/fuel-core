use std::sync::Arc;
use std::thread;
use std::time::Duration;
use std::{collections::HashSet, convert::TryFrom};

use graph::anyhow::bail;
use graph::components::store::{BlockStore as _, ChainStore as _};
use graph::prelude::{anyhow, BlockNumber, BlockPtr, NodeId, SubgraphStore};
use graph_store_postgres::BlockStore;
use graph_store_postgres::{connection_pool::ConnectionPool, Store};

use crate::manager::deployment::{Deployment, DeploymentSearch};

async fn block_ptr(
    store: Arc<BlockStore>,
    searches: &[DeploymentSearch],
    deployments: &[Deployment],
    hash: &str,
    number: BlockNumber,
    force: bool,
) -> Result<BlockPtr, anyhow::Error> {
    let block_ptr_to = BlockPtr::try_from((hash, number as i64))
        .map_err(|e| anyhow!("error converting to block pointer: {}", e))?;

    let chains = deployments.iter().map(|d| &d.chain).collect::<HashSet<_>>();
    if chains.len() > 1 {
        let names = searches
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        bail!("the deployments matching `{names}` are on different chains");
    }
    let chain = chains.iter().next().unwrap();
    let chain_store = match store.chain_store(chain) {
        None => bail!("can not find chain store for {}", chain),
        Some(store) => store,
    };
    if let Some((_, number, _)) = chain_store.block_number(&block_ptr_to.hash).await? {
        if number != block_ptr_to.number {
            bail!(
                "the given hash is for block number {} but the command specified block number {}",
                number,
                block_ptr_to.number
            );
        }
    } else if !force {
        bail!(
            "the chain {} does not have a block with hash {} \
               (run with --force to avoid this error)",
            chain,
            block_ptr_to.hash
        );
    }
    Ok(block_ptr_to)
}

pub async fn run(
    primary: ConnectionPool,
    store: Arc<Store>,
    searches: Vec<DeploymentSearch>,
    block_hash: Option<String>,
    block_number: Option<BlockNumber>,
    force: bool,
    sleep: Duration,
    start_block: bool,
) -> Result<(), anyhow::Error> {
    const PAUSED: &str = "paused_";

    // Sanity check
    if !start_block && (block_hash.is_none() || block_number.is_none()) {
        bail!("--block-hash and --block-number must be specified when --start-block is not set");
    }

    let subgraph_store = store.subgraph_store();
    let block_store = store.block_store();

    let deployments = searches
        .iter()
        .map(|search| search.lookup(&primary))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    if deployments.is_empty() {
        println!("nothing to do");
        return Ok(());
    }

    let block_ptr_to = if start_block {
        None
    } else {
        Some(
            block_ptr(
                block_store,
                &searches,
                &deployments,
                block_hash.as_deref().unwrap_or_default(),
                block_number.unwrap_or_default(),
                force,
            )
            .await?,
        )
    };

    println!("Pausing deployments");
    let mut paused = false;
    for deployment in &deployments {
        if let Some(node) = &deployment.node_id {
            if !node.starts_with(PAUSED) {
                let loc = deployment.locator();
                let node =
                    NodeId::new(format!("{}{}", PAUSED, node)).expect("paused_ node id is valid");
                subgraph_store.reassign_subgraph(&loc, &node)?;
                println!("  ... paused {}", loc);
                paused = true;
            }
        }
    }

    if paused {
        // There's no good way to tell that a subgraph has in fact stopped
        // indexing. We sleep and hope for the best.
        println!(
            "\nWaiting {}s to make sure pausing was processed",
            sleep.as_secs()
        );
        thread::sleep(sleep);
    }

    println!("\nRewinding deployments");
    for deployment in &deployments {
        let loc = deployment.locator();
        let block_store = store.block_store();
        let deployment_details = subgraph_store.load_deployment_by_id(loc.clone().into())?;
        let block_ptr_to = block_ptr_to.clone();

        let start_block = deployment_details.start_block.or_else(|| {
            block_store
                .chain_store(&deployment.chain)
                .and_then(|chain_store| chain_store.genesis_block_ptr().ok())
        });

        match (block_ptr_to, start_block) {
            (Some(block_ptr), _) => {
                subgraph_store.rewind(loc.hash.clone(), block_ptr)?;
                println!("  ... rewound {}", loc);
            }
            (None, Some(start_block_ptr)) => {
                subgraph_store.truncate(loc.hash.clone(), start_block_ptr)?;
                println!("  ... truncated {}", loc);
            }
            (None, None) => {
                println!("  ... Failed to find start block for {}", loc);
            }
        }
    }

    println!("Resuming deployments");
    for deployment in &deployments {
        if let Some(node) = &deployment.node_id {
            let loc = deployment.locator();
            let node = NodeId::new(node.clone()).expect("node id is valid");
            subgraph_store.reassign_subgraph(&loc, &node)?;
        }
    }
    Ok(())
}
