use std::sync::Arc;

use graph::blockchain::BlockPtr;
use graph::cheap_clone::CheapClone;
use graph::prelude::BlockNumber;
use graph::prelude::ChainStore as _;
use graph::prelude::EthereumBlock;
use graph::prelude::LightEthereumBlockExt as _;
use graph::prelude::{anyhow, anyhow::bail};
use graph::{
    components::store::BlockStore as _, prelude::anyhow::Error, prelude::serde_json as json,
};
use graph_store_postgres::BlockStore;
use graph_store_postgres::ChainStore;
use graph_store_postgres::{
    command_support::catalog::block_store, connection_pool::ConnectionPool,
};

pub async fn list(primary: ConnectionPool, store: Arc<BlockStore>) -> Result<(), Error> {
    let mut chains = {
        let conn = primary.get()?;
        block_store::load_chains(&conn)?
    };
    chains.sort_by_key(|chain| chain.name.clone());

    if !chains.is_empty() {
        println!(
            "{:^20} | {:^10} | {:^10} | {:^7} | {:^10}",
            "name", "shard", "namespace", "version", "head block"
        );
        println!(
            "{:-^20}-+-{:-^10}-+-{:-^10}-+-{:-^7}-+-{:-^10}",
            "", "", "", "", ""
        );
    }
    for chain in chains {
        let head_block = match store.chain_store(&chain.name) {
            None => "no chain".to_string(),
            Some(chain_store) => chain_store
                .chain_head_ptr()
                .await?
                .map(|ptr| ptr.number.to_string())
                .unwrap_or("none".to_string()),
        };
        println!(
            "{:<20} | {:<10} | {:<10} | {:>7} | {:>10}",
            chain.name, chain.shard, chain.storage, chain.net_version, head_block
        );
    }
    Ok(())
}

pub async fn clear_call_cache(
    chain_store: Arc<ChainStore>,
    from: i32,
    to: i32,
) -> Result<(), Error> {
    println!(
        "Removing entries for blocks from {from} to {to} from the call cache for `{}`",
        chain_store.chain
    );
    chain_store.clear_call_cache(from, to).await?;
    Ok(())
}

pub async fn info(
    primary: ConnectionPool,
    store: Arc<BlockStore>,
    name: String,
    offset: BlockNumber,
    hashes: bool,
) -> Result<(), Error> {
    fn row(label: &str, value: impl std::fmt::Display) {
        println!("{:<16} | {}", label, value);
    }

    fn print_ptr(label: &str, ptr: Option<BlockPtr>, hashes: bool) {
        match ptr {
            None => {
                row(label, "ø");
            }
            Some(ptr) => {
                row(label, ptr.number);
                if hashes {
                    row("", ptr.hash);
                }
            }
        }
    }

    let conn = primary.get()?;

    let chain =
        block_store::find_chain(&conn, &name)?.ok_or_else(|| anyhow!("unknown chain: {}", name))?;

    let chain_store = store
        .chain_store(&chain.name)
        .ok_or_else(|| anyhow!("unknown chain: {}", name))?;
    let head_block = chain_store.cheap_clone().chain_head_ptr().await?;
    let ancestor = match &head_block {
        None => None,
        Some(head_block) => chain_store
            .ancestor_block(head_block.clone(), offset)
            .await?
            .map(json::from_value::<EthereumBlock>)
            .transpose()?
            .map(|b| b.block.block_ptr()),
    };

    row("name", chain.name);
    row("shard", chain.shard);
    row("namespace", chain.storage);
    row("net_version", chain.net_version);
    if hashes {
        row("genesis", chain.genesis_block);
    }
    print_ptr("head block", head_block, hashes);
    row("reorg threshold", offset);
    print_ptr("reorg ancestor", ancestor, hashes);

    Ok(())
}

pub fn remove(primary: ConnectionPool, store: Arc<BlockStore>, name: String) -> Result<(), Error> {
    let sites = {
        let conn = graph_store_postgres::command_support::catalog::Connection::new(primary.get()?);
        conn.find_sites_for_network(&name)?
    };

    if !sites.is_empty() {
        println!(
            "there are {} deployments using chain {}:",
            sites.len(),
            name
        );
        for site in sites {
            println!("{:<8} | {} ", site.namespace, site.deployment);
        }
        bail!("remove all deployments using chain {} first", name);
    }

    store.drop_chain(&name)?;

    Ok(())
}
