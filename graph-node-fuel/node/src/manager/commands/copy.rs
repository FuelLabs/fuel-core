use diesel::{ExpressionMethods, JoinOnDsl, OptionalExtension, QueryDsl, RunQueryDsl};
use std::{collections::HashMap, sync::Arc, time::SystemTime};

use graph::{
    components::store::{BlockStore as _, DeploymentId},
    data::query::QueryTarget,
    prelude::{
        anyhow::{anyhow, bail, Error},
        chrono::{DateTime, Duration, SecondsFormat, Utc},
        BlockPtr, ChainStore, DeploymentHash, NodeId, QueryStoreManager,
    },
};
use graph_store_postgres::{
    command_support::{
        catalog::{self, copy_state, copy_table_state},
        on_sync, OnSync,
    },
    PRIMARY_SHARD,
};
use graph_store_postgres::{connection_pool::ConnectionPool, Shard, Store, SubgraphStore};

use crate::manager::deployment::DeploymentSearch;
use crate::manager::display::List;

type UtcDateTime = DateTime<Utc>;

#[derive(Queryable, QueryableByName, Debug)]
#[table_name = "copy_state"]
struct CopyState {
    src: i32,
    dst: i32,
    #[allow(dead_code)]
    target_block_hash: Vec<u8>,
    target_block_number: i32,
    started_at: UtcDateTime,
    finished_at: Option<UtcDateTime>,
    cancelled_at: Option<UtcDateTime>,
}

#[derive(Queryable, QueryableByName, Debug)]
#[table_name = "copy_table_state"]
struct CopyTableState {
    #[allow(dead_code)]
    id: i32,
    entity_type: String,
    #[allow(dead_code)]
    dst: i32,
    next_vid: i64,
    target_vid: i64,
    batch_size: i64,
    #[allow(dead_code)]
    started_at: UtcDateTime,
    finished_at: Option<UtcDateTime>,
    duration_ms: i64,
}

impl CopyState {
    fn find(
        pools: &HashMap<Shard, ConnectionPool>,
        shard: &Shard,
        dst: i32,
    ) -> Result<Option<(CopyState, Vec<CopyTableState>, OnSync)>, Error> {
        use copy_state as cs;
        use copy_table_state as cts;

        let dpool = pools
            .get(shard)
            .ok_or_else(|| anyhow!("can not find pool for shard {}", shard))?;

        let dconn = dpool.get()?;

        let tables = cts::table
            .filter(cts::dst.eq(dst))
            .order_by(cts::entity_type)
            .load::<CopyTableState>(&dconn)?;

        let on_sync = on_sync(&dconn, DeploymentId(dst))?;

        Ok(cs::table
            .filter(cs::dst.eq(dst))
            .get_result::<CopyState>(&dconn)
            .optional()?
            .map(|state| (state, tables, on_sync)))
    }
}

pub async fn create(
    store: Arc<Store>,
    primary: ConnectionPool,
    src: DeploymentSearch,
    shard: String,
    shards: Vec<String>,
    node: String,
    block_offset: u32,
    activate: bool,
    replace: bool,
) -> Result<(), Error> {
    let block_offset = block_offset as i32;
    let on_sync = match (activate, replace) {
        (true, true) => bail!("--activate and --replace can't both be specified"),
        (true, false) => OnSync::Activate,
        (false, true) => OnSync::Replace,
        (false, false) => OnSync::None,
    };

    let subgraph_store = store.subgraph_store();
    let src = src.locate_unique(&primary)?;
    let query_store = store
        .query_store(
            QueryTarget::Deployment(src.hash.clone(), Default::default()),
            true,
        )
        .await?;
    let network = query_store.network_name();

    let src_ptr = query_store.block_ptr().await?.ok_or_else(|| anyhow!("subgraph {} has not indexed any blocks yet and can not be used as the source of a copy", src))?;
    let src_number = if src_ptr.number <= block_offset {
        bail!("subgraph {} has only indexed up to block {}, but we need at least block {} before we can copy from it", src, src_ptr.number, block_offset);
    } else {
        src_ptr.number - block_offset
    };

    let chain_store = store
        .block_store()
        .chain_store(network)
        .ok_or_else(|| anyhow!("could not find chain store for network {}", network))?;
    let mut hashes = chain_store.block_hashes_by_block_number(src_number)?;
    let hash = match hashes.len() {
        0 => bail!(
            "could not find a block with number {} in our cache",
            src_number
        ),
        1 => hashes.pop().unwrap(),
        n => bail!(
            "the cache contains {} hashes for block number {}",
            n,
            src_number
        ),
    };
    let base_ptr = BlockPtr::new(hash, src_number);

    if !shards.contains(&shard) {
        bail!(
            "unknown shard {shard}, only shards {} are configured",
            shards.join(", ")
        )
    }
    let shard = Shard::new(shard)?;
    let node = NodeId::new(node.clone()).map_err(|()| anyhow!("invalid node id `{}`", node))?;

    let dst = subgraph_store.copy_deployment(&src, shard, node, base_ptr, on_sync)?;

    println!("created deployment {} as copy of {}", dst, src);
    Ok(())
}

pub fn activate(store: Arc<SubgraphStore>, deployment: String, shard: String) -> Result<(), Error> {
    let shard = Shard::new(shard)?;
    let deployment =
        DeploymentHash::new(deployment).map_err(|s| anyhow!("illegal deployment hash `{}`", s))?;
    let deployment = store
        .locate_in_shard(&deployment, shard.clone())?
        .ok_or_else(|| {
            anyhow!(
                "could not find a copy for {} in shard {}",
                deployment,
                shard
            )
        })?;
    store.activate(&deployment)?;
    println!("activated copy {}", deployment);
    Ok(())
}

pub fn list(pools: HashMap<Shard, ConnectionPool>) -> Result<(), Error> {
    use catalog::active_copies as ac;
    use catalog::deployment_schemas as ds;

    let primary = pools.get(&*PRIMARY_SHARD).expect("there is a primary pool");
    let conn = primary.get()?;

    let copies = ac::table
        .inner_join(ds::table.on(ds::id.eq(ac::dst)))
        .select((
            ac::src,
            ac::dst,
            ac::cancelled_at,
            ac::queued_at,
            ds::subgraph,
            ds::shard,
        ))
        .load::<(i32, i32, Option<UtcDateTime>, UtcDateTime, String, Shard)>(&conn)?;
    if copies.is_empty() {
        println!("no active copies");
    } else {
        fn status(name: &str, at: UtcDateTime) {
            println!(
                "{:20} | {}",
                name,
                at.to_rfc3339_opts(SecondsFormat::Secs, false)
            );
        }

        for (src, dst, cancelled_at, queued_at, deployment_hash, shard) in copies {
            println!("{:-<78}", "");

            println!("{:20} | {}", "deployment", deployment_hash);
            println!("{:20} | sgd{} -> sgd{} ({})", "action", src, dst, shard);
            match CopyState::find(&pools, &shard, dst)? {
                Some((state, tables, _)) => match cancelled_at {
                    Some(cancel_requested) => match state.cancelled_at {
                        Some(cancelled_at) => status("cancelled", cancelled_at),
                        None => status("cancel requested", cancel_requested),
                    },
                    None => match state.finished_at {
                        Some(finished_at) => status("finished", finished_at),
                        None => {
                            let target: i64 = tables.iter().map(|table| table.target_vid).sum();
                            let next: i64 = tables.iter().map(|table| table.next_vid).sum();
                            let done = next as f64 / target as f64 * 100.0;
                            status("started", state.started_at);
                            println!("{:20} | {:.2}% done, {}/{}", "progress", done, next, target)
                        }
                    },
                },
                None => status("queued", queued_at),
            };
        }
    }
    Ok(())
}

pub fn status(pools: HashMap<Shard, ConnectionPool>, dst: &DeploymentSearch) -> Result<(), Error> {
    use catalog::active_copies as ac;
    use catalog::deployment_schemas as ds;

    fn done(ts: &Option<UtcDateTime>) -> String {
        ts.map(|_| "✓").unwrap_or(".").to_string()
    }

    fn duration(start: &UtcDateTime, end: &Option<UtcDateTime>) -> String {
        let start = *start;
        let end = *end;

        let end = end.unwrap_or(UtcDateTime::from(SystemTime::now()));
        let duration = end - start;

        human_duration(duration)
    }

    fn human_duration(duration: Duration) -> String {
        if duration.num_seconds() < 5 {
            format!("{}ms", duration.num_milliseconds())
        } else if duration.num_minutes() < 5 {
            format!("{}s", duration.num_seconds())
        } else {
            format!("{}m", duration.num_minutes())
        }
    }

    let primary = pools
        .get(&*PRIMARY_SHARD)
        .ok_or_else(|| anyhow!("can not find deployment with id {}", dst))?;
    let pconn = primary.get()?;
    let dst = dst.locate_unique(primary)?.id.0;

    let (shard, deployment) = ds::table
        .filter(ds::id.eq(dst))
        .select((ds::shard, ds::subgraph))
        .get_result::<(Shard, String)>(&pconn)?;

    let (active, cancelled_at) = ac::table
        .filter(ac::dst.eq(dst))
        .select((ac::src, ac::cancelled_at))
        .get_result::<(i32, Option<UtcDateTime>)>(&pconn)
        .optional()?
        .map(|(_, cancelled_at)| (true, cancelled_at))
        .unwrap_or((false, None));

    let (state, tables, on_sync) = match CopyState::find(&pools, &shard, dst)? {
        Some((state, tables, on_sync)) => (state, tables, on_sync),
        None => {
            if active {
                println!("copying is queued but has not started");
                return Ok(());
            } else {
                bail!("no copy operation for {} exists", dst);
            }
        }
    };

    let progress = match &state.finished_at {
        Some(_) => done(&state.finished_at),
        None => {
            let target: i64 = tables.iter().map(|table| table.target_vid).sum();
            let next: i64 = tables.iter().map(|table| table.next_vid).sum();
            let pct = next as f64 / target as f64 * 100.0;
            format!("{:.2}% done, {}/{}", pct, next, target)
        }
    };

    let mut lst = vec![
        "deployment",
        "src",
        "dst",
        "target block",
        "on sync",
        "duration",
        "status",
    ];
    let mut vals = vec![
        deployment,
        state.src.to_string(),
        state.dst.to_string(),
        state.target_block_number.to_string(),
        on_sync.to_str().to_string(),
        duration(&state.started_at, &state.finished_at),
        progress,
    ];
    match (cancelled_at, state.cancelled_at) {
        (Some(c), None) => {
            lst.push("cancel");
            vals.push(format!("requested at {}", c));
        }
        (_, Some(c)) => {
            lst.push("cancel");
            vals.push(format!("cancelled at {}", c));
        }
        (None, None) => {}
    }
    let mut lst = List::new(lst);
    lst.append(vals);
    lst.render();
    println!();

    println!(
        "{:^30} | {:^8} | {:^8} | {:^8} | {:^8}",
        "entity type", "next", "target", "batch", "duration"
    );
    println!("{:-<74}", "-");
    for table in tables {
        let status = if table.next_vid > 0 && table.next_vid < table.target_vid {
            ">".to_string()
        } else if table.target_vid < 0 {
            // empty source table
            "✓".to_string()
        } else {
            done(&table.finished_at)
        };
        println!(
            "{} {:<28} | {:>8} | {:>8} | {:>8} | {:>8}",
            status,
            table.entity_type,
            table.next_vid,
            table.target_vid,
            table.batch_size,
            human_duration(Duration::milliseconds(table.duration_ms)),
        );
    }

    Ok(())
}
