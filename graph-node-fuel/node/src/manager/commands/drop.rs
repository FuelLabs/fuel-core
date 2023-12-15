use crate::manager::{
    deployment::{Deployment, DeploymentSearch},
    display::List,
    prompt::prompt_for_confirmation,
};
use graph::anyhow::{self, bail};
use graph_store_postgres::{connection_pool::ConnectionPool, NotificationSender, SubgraphStore};
use std::sync::Arc;

/// Finds, unassigns, record and remove matching deployments.
///
/// Asks for confirmation before removing any data.
/// This is a convenience fuction that to call a series of other graphman commands.
pub async fn run(
    primary_pool: ConnectionPool,
    subgraph_store: Arc<SubgraphStore>,
    sender: Arc<NotificationSender>,
    search_term: DeploymentSearch,
    current: bool,
    pending: bool,
    used: bool,
    skip_confirmation: bool,
) -> anyhow::Result<()> {
    // call `graphman info` to find matching deployments
    let deployments = search_term.find(primary_pool.clone(), current, pending, used)?;
    if deployments.is_empty() {
        bail!("Found no deployment for search_term: {search_term}")
    } else {
        print_deployments(&deployments);
        if !skip_confirmation && !prompt_for_confirmation("\nContinue?")? {
            println!("Execution aborted by user");
            return Ok(());
        }
    }
    // call `graphman unassign` to stop any active deployments
    crate::manager::commands::assign::unassign(primary_pool, &sender, &search_term).await?;

    // call `graphman remove` to unregister the subgraph's name
    for deployment in &deployments {
        crate::manager::commands::remove::run(subgraph_store.clone(), &deployment.name)?;
    }

    // call `graphman unused record` to register those deployments unused
    crate::manager::commands::unused_deployments::record(subgraph_store.clone())?;

    // call `graphman unused remove` to remove each deployment's data
    for deployment in &deployments {
        crate::manager::commands::unused_deployments::remove(
            subgraph_store.clone(),
            1_000_000,
            Some(&deployment.deployment),
            None,
        )?;
    }
    Ok(())
}

fn print_deployments(deployments: &[Deployment]) {
    let mut list = List::new(vec!["name", "deployment"]);
    println!("Found {} deployment(s) to remove:", deployments.len());
    for deployment in deployments {
        list.append(vec![
            deployment.name.to_string(),
            deployment.deployment.to_string(),
        ]);
    }
    list.render();
}
