use std::sync::Arc;

use graph::prelude::{
    anyhow::{anyhow, bail, Result},
    reqwest,
    serde_json::{json, Value},
    SubgraphName, SubgraphStore,
};

use crate::manager::deployment::DeploymentSearch;

// Function to send an RPC request and handle errors
async fn send_rpc_request(url: &str, payload: Value) -> Result<()> {
    let client = reqwest::Client::new();
    let response = client.post(url).json(&payload).send().await?;

    if response.status().is_success() {
        Ok(())
    } else {
        Err(response
            .error_for_status()
            .expect_err("Failed to parse error response")
            .into())
    }
}

// Function to send subgraph_create request
async fn send_create_request(name: &str, url: &str) -> Result<()> {
    // Construct the JSON payload for subgraph_create
    let create_payload = json!({
        "jsonrpc": "2.0",
        "method": "subgraph_create",
        "params": {
            "name": name,
        },
        "id": "1"
    });

    // Send the subgraph_create request
    send_rpc_request(url, create_payload)
        .await
        .map_err(|e| e.context(format!("Failed to create subgraph with name `{}`", name)))
}

// Function to send subgraph_deploy request
async fn send_deploy_request(name: &str, deployment: &str, url: &str) -> Result<()> {
    // Construct the JSON payload for subgraph_deploy
    let deploy_payload = json!({
        "jsonrpc": "2.0",
        "method": "subgraph_deploy",
        "params": {
            "name": name,
            "ipfs_hash": deployment,
        },
        "id": "1"
    });

    // Send the subgraph_deploy request
    send_rpc_request(url, deploy_payload).await.map_err(|e| {
        e.context(format!(
            "Failed to deploy subgraph `{}` to `{}`",
            deployment, name
        ))
    })
}
pub async fn run(
    subgraph_store: Arc<impl SubgraphStore>,
    deployment: DeploymentSearch,
    search: DeploymentSearch,
    url: String,
    create: bool,
) -> Result<()> {
    let hash = match deployment {
        DeploymentSearch::Hash { hash, shard: _ } => hash,
        _ => bail!("The `deployment` argument must be a valid IPFS hash"),
    };

    let name = match search {
        DeploymentSearch::Name { name } => name,
        _ => bail!("The `name` must be a valid subgraph name"),
    };

    if create {
        println!("Creating subgraph `{}`", name);
        let subgraph_name =
            SubgraphName::new(name.clone()).map_err(|_| anyhow!("Invalid subgraph name"))?;

        let exists = subgraph_store.subgraph_exists(&subgraph_name)?;

        if exists {
            bail!("Subgraph with name `{}` already exists", name);
        }

        // Send the subgraph_create request
        send_create_request(&name, &url).await?;
        println!("Subgraph `{}` created", name);
    }

    // Send the subgraph_deploy request
    println!("Deploying subgraph `{}` to `{}`", hash, name);
    send_deploy_request(&name, &hash, &url).await?;
    println!("Subgraph `{}` deployed to `{}`", name, url);

    Ok(())
}
