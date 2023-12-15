use crate::manager::prompt::prompt_for_confirmation;
use graph::{
    anyhow::{bail, ensure},
    components::store::ChainStore as ChainStoreTrait,
    prelude::{
        anyhow::{self, anyhow, Context},
        web3::types::H256,
    },
    slog::Logger,
};
use graph_chain_ethereum::{EthereumAdapter, EthereumAdapterTrait};
use graph_store_postgres::ChainStore;
use std::sync::Arc;

pub async fn by_hash(
    hash: &str,
    chain_store: Arc<ChainStore>,
    ethereum_adapter: &EthereumAdapter,
    logger: &Logger,
) -> anyhow::Result<()> {
    let block_hash = helpers::parse_block_hash(hash)?;
    run(&block_hash, &chain_store, ethereum_adapter, logger).await
}

pub async fn by_number(
    number: i32,
    chain_store: Arc<ChainStore>,
    ethereum_adapter: &EthereumAdapter,
    logger: &Logger,
    delete_duplicates: bool,
) -> anyhow::Result<()> {
    let block_hashes = steps::resolve_block_hash_from_block_number(number, &chain_store)?;

    match &block_hashes.as_slice() {
        [] => bail!("Could not find a block with number {} in store", number),
        [block_hash] => run(block_hash, &chain_store, ethereum_adapter, logger).await,
        &block_hashes => {
            handle_multiple_block_hashes(number, block_hashes, &chain_store, delete_duplicates)
                .await
        }
    }
}

pub async fn by_range(
    chain_store: Arc<ChainStore>,
    ethereum_adapter: &EthereumAdapter,
    range_from: Option<i32>,
    range_to: Option<i32>,
    logger: &Logger,
    delete_duplicates: bool,
) -> anyhow::Result<()> {
    // Resolve a range of block numbers into a collection of blocks hashes
    let range = ranges::Range::new(range_from, range_to)?;
    let max = match range.upper_bound {
        // When we have an open upper bound, we use the chain head's block number
        None => steps::find_chain_head(&chain_store)?,
        Some(x) => x,
    };
    // FIXME: This performs poorly.
    // TODO: This could be turned into async code
    for block_number in range.lower_bound..=max {
        println!("Checking block [{block_number}/{max}]");
        let block_hashes = steps::resolve_block_hash_from_block_number(block_number, &chain_store)?;
        match &block_hashes.as_slice() {
            [] => eprintln!("Found no block hash with number {block_number}"),
            [block_hash] => run(block_hash, &chain_store, ethereum_adapter, logger).await?,
            &block_hashes => {
                handle_multiple_block_hashes(
                    block_number,
                    block_hashes,
                    &chain_store,
                    delete_duplicates,
                )
                .await?
            }
        }
    }
    Ok(())
}

pub fn truncate(chain_store: Arc<ChainStore>, skip_confirmation: bool) -> anyhow::Result<()> {
    let prompt = format!(
        "This will delete all cached blocks for {}.\nProceed?",
        chain_store.chain
    );
    if !skip_confirmation && !prompt_for_confirmation(&prompt)? {
        println!("Aborting.");
        return Ok(());
    }

    chain_store
        .truncate_block_cache()
        .with_context(|| format!("Failed to truncate block cache for {}", chain_store.chain))
}

async fn run(
    block_hash: &H256,
    chain_store: &ChainStore,
    ethereum_adapter: &EthereumAdapter,
    logger: &Logger,
) -> anyhow::Result<()> {
    let cached_block = steps::fetch_single_cached_block(*block_hash, chain_store)?;
    let provider_block =
        steps::fetch_single_provider_block(block_hash, ethereum_adapter, logger).await?;
    let diff = steps::diff_block_pair(&cached_block, &provider_block);
    steps::report_difference(diff.as_deref(), block_hash);
    if diff.is_some() {
        steps::delete_block(block_hash, chain_store)?;
    }
    Ok(())
}

async fn handle_multiple_block_hashes(
    block_number: i32,
    block_hashes: &[H256],
    chain_store: &ChainStore,
    delete_duplicates: bool,
) -> anyhow::Result<()> {
    println!(
        "graphman found {} different block hashes for block number {} in the store \
         and is unable to tell which one to check:",
        block_hashes.len(),
        block_number
    );
    for (num, hash) in block_hashes.iter().enumerate() {
        println!("{:>4}:  {hash:?}", num + 1);
    }
    if delete_duplicates {
        println!("Deleting duplicated blocks...");
        for hash in block_hashes {
            steps::delete_block(hash, chain_store)?;
        }
    } else {
        eprintln!(
            "Operation aborted for block number {block_number}.\n\
             To delete the duplicated blocks and continue this operation, rerun this command with \
             the `--delete-duplicates` option."
        )
    }
    Ok(())
}

mod steps {
    use super::*;

    use futures::compat::Future01CompatExt;
    use graph::{
        anyhow::bail,
        prelude::serde_json::{self, Value},
    };
    use json_structural_diff::{colorize as diff_to_string, JsonDiff};

    /// Queries the [`ChainStore`] about the block hash for the given block number.
    ///
    /// Multiple block hashes can be returned as the store does not enforce uniqueness based on
    /// block numbers.
    /// Returns an empty vector if no block hash is found.
    pub(super) fn resolve_block_hash_from_block_number(
        number: i32,
        chain_store: &ChainStore,
    ) -> anyhow::Result<Vec<H256>> {
        let block_hashes = chain_store.block_hashes_by_block_number(number)?;
        Ok(block_hashes
            .into_iter()
            .map(|x| H256::from_slice(&x.as_slice()[..32]))
            .collect())
    }

    /// Queries the [`ChainStore`] for a cached block given a block hash.
    ///
    /// Errors on a non-unary result.
    pub(super) fn fetch_single_cached_block(
        block_hash: H256,
        chain_store: &ChainStore,
    ) -> anyhow::Result<Value> {
        let blocks = chain_store.blocks(&[block_hash.into()])?;
        match blocks.len() {
            0 => bail!("Failed to locate block with hash {} in store", block_hash),
            1 => {}
            _ => bail!("Found multiple blocks with hash {} in store", block_hash),
        };
        // Unwrap: We just checked that the vector has a single element
        Ok(blocks.into_iter().next().unwrap())
    }

    /// Fetches a block from a JRPC endpoint.
    ///
    /// Errors on provider failure or if the returned block has a different hash than the one
    /// requested.
    pub(super) async fn fetch_single_provider_block(
        block_hash: &H256,
        ethereum_adapter: &EthereumAdapter,
        logger: &Logger,
    ) -> anyhow::Result<Value> {
        let provider_block = ethereum_adapter
            .block_by_hash(logger, *block_hash)
            .compat()
            .await
            .with_context(|| format!("failed to fetch block {block_hash}"))?
            .ok_or_else(|| anyhow!("JRPC provider found no block with hash {block_hash:?}"))?;
        ensure!(
            provider_block.hash == Some(*block_hash),
            "Provider responded with a different block hash"
        );
        serde_json::to_value(provider_block)
            .context("failed to parse provider block as a JSON value")
    }

    /// Compares two [`serde_json::Value`] values.
    ///
    /// If they are different, returns a user-friendly string ready to be displayed.
    pub(super) fn diff_block_pair(a: &Value, b: &Value) -> Option<String> {
        if a == b {
            None
        } else {
            match JsonDiff::diff(a, b, false).diff {
                // The diff could potentially be a `Value::Null`, which is equivalent to not being
                // different at all.
                None | Some(Value::Null) => None,
                Some(diff) => {
                    // Convert the JSON diff to a pretty-formatted text that will be displayed to
                    // the user
                    Some(diff_to_string(&diff, false))
                }
            }
        }
    }

    /// Prints the difference between two [`serde_json::Value`] values to the user.
    pub(super) fn report_difference(difference: Option<&str>, hash: &H256) {
        if let Some(diff) = difference {
            eprintln!("block {hash} diverges from cache:");
            eprintln!("{diff}");
        } else {
            println!("Cached block is equal to the same block from provider.")
        }
    }

    /// Attempts to delete a block from the block cache.
    pub(super) fn delete_block(hash: &H256, chain_store: &ChainStore) -> anyhow::Result<()> {
        println!("Deleting block {hash} from cache.");
        chain_store.delete_blocks(&[hash])?;
        println!("Done.");
        Ok(())
    }

    /// Queries the [`ChainStore`] about the chain head.
    pub(super) fn find_chain_head(chain_store: &ChainStore) -> anyhow::Result<i32> {
        let chain_head: Option<i32> = chain_store.chain_head_block(&chain_store.chain)?;
        chain_head.ok_or_else(|| anyhow!("Could not find the chain head for {}", chain_store.chain))
    }
}

mod helpers {
    use super::*;
    use graph::prelude::hex;

    /// Tries to parse a [`H256`] from a hex string.
    pub(super) fn parse_block_hash(hash: &str) -> anyhow::Result<H256> {
        let hash = hash.trim_start_matches("0x");
        let hash = hex::decode(hash)?;
        Ok(H256::from_slice(&hash))
    }
}

/// Custom range type
mod ranges {
    use graph::prelude::anyhow::{self, bail};

    pub(super) struct Range {
        pub(super) lower_bound: i32,
        pub(super) upper_bound: Option<i32>,
    }

    impl Range {
        pub fn new(lower_bound: Option<i32>, upper_bound: Option<i32>) -> anyhow::Result<Self> {
            let (lower_bound, upper_bound) = match (lower_bound, upper_bound) {
                // Invalid cases:
                (None, None) => {
                    bail!(
                        "This would wipe the whole cache. \
                         Use `graphman chain truncate` instead"
                    )
                }
                (Some(0), _) => bail!("Genesis block can't be removed"),
                (Some(x), _) | (_, Some(x)) if x < 0 => {
                    bail!("Negative block number used as range bound: {}", x)
                }
                (Some(lower), Some(upper)) if upper < lower => bail!(
                    "Upper bound ({}) can't be smaller than lower bound ({})",
                    upper,
                    lower
                ),

                // Valid cases:
                // Open lower bounds are set to the lowest possible block number
                (None, upper @ Some(_)) => (1, upper),
                (Some(lower), upper) => (lower, upper),
            };

            Ok(Self {
                lower_bound,
                upper_bound,
            })
        }
    }
}
