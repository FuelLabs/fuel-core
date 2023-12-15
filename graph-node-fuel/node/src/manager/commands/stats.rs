use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;

use crate::manager::deployment::DeploymentSearch;
use diesel::r2d2::ConnectionManager;
use diesel::r2d2::PooledConnection;
use diesel::PgConnection;
use graph::components::store::DeploymentLocator;
use graph::components::store::VersionStats;
use graph::prelude::anyhow;
use graph_store_postgres::command_support::catalog as store_catalog;
use graph_store_postgres::command_support::catalog::Site;
use graph_store_postgres::connection_pool::ConnectionPool;
use graph_store_postgres::Shard;
use graph_store_postgres::SubgraphStore;
use graph_store_postgres::PRIMARY_SHARD;

fn site_and_conn(
    pools: HashMap<Shard, ConnectionPool>,
    search: &DeploymentSearch,
) -> Result<(Site, PooledConnection<ConnectionManager<PgConnection>>), anyhow::Error> {
    let primary_pool = pools.get(&*PRIMARY_SHARD).unwrap();
    let locator = search.locate_unique(primary_pool)?;

    let conn = primary_pool.get()?;
    let conn = store_catalog::Connection::new(conn);

    let site = conn
        .locate_site(locator)?
        .ok_or_else(|| anyhow!("deployment `{}` does not exist", search))?;

    let conn = pools.get(&site.shard).unwrap().get()?;

    Ok((site, conn))
}

pub async fn account_like(
    store: Arc<SubgraphStore>,
    primary_pool: ConnectionPool,
    clear: bool,
    search: &DeploymentSearch,
    table: String,
) -> Result<(), anyhow::Error> {
    let locator = search.locate_unique(&primary_pool)?;

    store.set_account_like(&locator, &table, !clear).await?;
    let clear_text = if clear { "cleared" } else { "set" };
    println!("{}: account-like flag {}", table, clear_text);

    Ok(())
}

pub fn abbreviate_table_name(table: &str, size: usize) -> String {
    if table.len() > size {
        let fragment = size / 2 - 2;
        let last = table.len() - fragment;
        let mut table = table.to_string();
        table.replace_range(fragment..last, "..");
        let table = table.trim().to_string();
        table
    } else {
        table.to_string()
    }
}

pub fn show_stats(
    stats: &[VersionStats],
    account_like: HashSet<String>,
) -> Result<(), anyhow::Error> {
    fn header() {
        println!(
            "{:^30} | {:^10} | {:^10} | {:^7}",
            "table", "entities", "versions", "ratio"
        );
        println!("{:-^30}-+-{:-^10}-+-{:-^10}-+-{:-^7}", "", "", "", "");
    }

    fn footer() {
        println!("  (a): account-like flag set");
    }

    fn print_stats(s: &VersionStats, account_like: bool) {
        println!(
            "{:<26} {:3} | {:>10} | {:>10} | {:>5.1}%",
            abbreviate_table_name(&s.tablename, 26),
            if account_like { "(a)" } else { "   " },
            s.entities,
            s.versions,
            s.ratio * 100.0
        );
    }

    header();
    for s in stats {
        print_stats(s, account_like.contains(&s.tablename));
    }
    if !account_like.is_empty() {
        footer();
    }

    Ok(())
}

pub fn show(
    pools: HashMap<Shard, ConnectionPool>,
    search: &DeploymentSearch,
) -> Result<(), anyhow::Error> {
    let (site, conn) = site_and_conn(pools, search)?;

    let stats = store_catalog::stats(&conn, &site)?;

    let account_like = store_catalog::account_like(&conn, &site)?;

    show_stats(stats.as_slice(), account_like)
}

pub fn analyze(
    store: Arc<SubgraphStore>,
    pool: ConnectionPool,
    search: DeploymentSearch,
    entity_name: Option<&str>,
) -> Result<(), anyhow::Error> {
    let locator = search.locate_unique(&pool)?;
    analyze_loc(store, &locator, entity_name)
}

fn analyze_loc(
    store: Arc<SubgraphStore>,
    locator: &DeploymentLocator,
    entity_name: Option<&str>,
) -> Result<(), anyhow::Error> {
    match entity_name {
        Some(entity_name) => println!("Analyzing table sgd{}.{entity_name}", locator.id),
        None => println!("Analyzing all tables for sgd{}", locator.id),
    }
    store.analyze(locator, entity_name).map_err(|e| anyhow!(e))
}

pub fn target(
    store: Arc<SubgraphStore>,
    primary: ConnectionPool,
    search: &DeploymentSearch,
) -> Result<(), anyhow::Error> {
    let locator = search.locate_unique(&primary)?;
    let (default, targets) = store.stats_targets(&locator)?;

    let has_targets = targets
        .values()
        .any(|cols| cols.values().any(|target| *target > 0));

    if has_targets {
        println!(
            "{:^74}",
            format!(
                "Statistics targets for sgd{} (default: {default})",
                locator.id
            )
        );
        println!("{:^30} | {:^30} | {:^8}", "table", "column", "target");
        println!("{:-^30}-+-{:-^30}-+-{:-^8}", "", "", "");
        for (table, columns) in targets {
            for (column, target) in columns {
                if target > 0 {
                    println!("{:<30} | {:<30} | {:>8}", table, column, target);
                }
            }
        }
    } else {
        println!(
            "no statistics targets set for sgd{}, global default is {default}",
            locator.id
        );
    }
    Ok(())
}

pub fn set_target(
    store: Arc<SubgraphStore>,
    primary: ConnectionPool,
    search: &DeploymentSearch,
    entity: Option<&str>,
    columns: Vec<String>,
    target: i32,
    no_analyze: bool,
) -> Result<(), anyhow::Error> {
    let columns = if columns.is_empty() {
        vec!["id".to_string(), "block_range".to_string()]
    } else {
        columns
    };

    let locator = search.locate_unique(&primary)?;

    store.set_stats_target(&locator, entity, columns, target)?;

    if !no_analyze {
        analyze_loc(store, &locator, entity)?;
    }
    Ok(())
}
