use std::{io::Write, time::Instant};

use graph::prelude::anyhow;
use graph_store_postgres::connection_pool::PoolCoordinator;

pub async fn remap(
    coord: &PoolCoordinator,
    src: Option<String>,
    dst: Option<String>,
    force: bool,
) -> Result<(), anyhow::Error> {
    let pools = {
        let mut pools = coord.pools();
        pools.sort_by(|pool1, pool2| pool1.shard.as_str().cmp(pool2.shard.as_str()));
        pools
    };
    let servers = coord.servers();

    if let Some(src) = &src {
        if !servers.iter().any(|srv| srv.shard.as_str() == src) {
            return Err(anyhow!("unknown source shard {src}"));
        }
    }
    if let Some(dst) = &dst {
        if !pools.iter().any(|pool| pool.shard.as_str() == dst) {
            return Err(anyhow!("unknown destination shard {dst}"));
        }
    }

    let servers = servers.iter().filter(|srv| match &src {
        None => true,
        Some(src) => srv.shard.as_str() == src,
    });

    for server in servers {
        let pools = pools.iter().filter(|pool| match &dst {
            None => true,
            Some(dst) => pool.shard.as_str() == dst,
        });

        for pool in pools {
            let start = Instant::now();
            print!(
                "Remapping imports from {} in shard {}",
                server.shard, pool.shard
            );
            std::io::stdout().flush().ok();
            if let Err(e) = pool.remap(server) {
                println!(" FAILED");
                println!("  error: {e}");
                if !force {
                    return Ok(());
                }
            } else {
                println!(" (done in {}s)", start.elapsed().as_secs());
            }
        }
    }
    Ok(())
}
