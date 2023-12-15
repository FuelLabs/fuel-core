use std::sync::Arc;

use graph::{components::store::StatusStore, data::subgraph::status, prelude::anyhow};
use graph_store_postgres::{connection_pool::ConnectionPool, Store};

use crate::manager::deployment::{Deployment, DeploymentSearch};

pub fn run(
    pool: ConnectionPool,
    store: Option<Arc<Store>>,
    search: DeploymentSearch,
    current: bool,
    pending: bool,
    used: bool,
) -> Result<(), anyhow::Error> {
    let deployments = search.find(pool, current, pending, used)?;
    let ids: Vec<_> = deployments.iter().map(|d| d.locator().id).collect();
    let statuses = match store {
        Some(store) => store.status(status::Filter::DeploymentIds(ids))?,
        None => vec![],
    };

    if deployments.is_empty() {
        println!("No matches");
    } else {
        Deployment::print_table(deployments, statuses);
    }
    Ok(())
}
