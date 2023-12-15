use std::{
    io::{Read as _, Write as _},
    time::{Duration, Instant},
};

use anyhow::anyhow;

use graph::prelude::serde_json::{self, Value};
use serde::Deserialize;
use tokio::{process::Command, time::sleep};

use crate::{
    contract::Contract,
    helpers::{graphql_query, graphql_query_with_vars, run_checked, TestFile},
    CONFIG,
};

#[derive(Clone, Debug)]
pub struct Subgraph {
    pub name: String,
    pub deployment: String,
    pub synced: bool,
    pub healthy: bool,
}

impl Subgraph {
    fn dir(name: &str) -> TestFile {
        TestFile::new(&format!("integration-tests/{name}"))
    }

    pub async fn patch(dir: &TestFile, contracts: &[Contract]) -> anyhow::Result<()> {
        let mut orig = String::new();
        dir.append("subgraph.yaml")
            .read()?
            .read_to_string(&mut orig)
            .unwrap();
        for contract in contracts {
            let repl = format!("@{}@", contract.name);
            let addr = format!("{:?}", contract.address);
            orig = orig.replace(&repl, &addr);
        }

        let mut patched = dir.append("subgraph.yaml.patched").create();
        patched.write_all(orig.as_bytes()).unwrap();
        Ok(())
    }

    /// Deploy the subgraph by running the required `graph` commands
    pub async fn deploy(name: &str, contracts: &[Contract]) -> anyhow::Result<String> {
        let dir = Self::dir(name);
        let name = format!("test/{name}");

        Self::patch(&dir, contracts).await?;

        // graph create --node <node> <name>
        let mut prog = Command::new(&CONFIG.graph_cli);
        let cmd = prog
            .arg("create")
            .arg("--node")
            .arg(CONFIG.graph_node.admin_uri())
            .arg(&name)
            .current_dir(&dir.path);
        run_checked(cmd).await?;

        // graph codegen subgraph.yaml
        let mut prog = Command::new(&CONFIG.graph_cli);
        let cmd = prog
            .arg("codegen")
            .arg("subgraph.yaml.patched")
            .current_dir(&dir.path);
        run_checked(cmd).await?;

        // graph deploy --node <node> --version-label v0.0.1 --ipfs <ipfs> <name> subgraph.yaml
        let mut prog = Command::new(&CONFIG.graph_cli);
        let cmd = prog
            .arg("deploy")
            .arg("--node")
            .arg(CONFIG.graph_node.admin_uri())
            .arg("--version-label")
            .arg("v0.0.1")
            .arg("--ipfs")
            .arg(&CONFIG.graph_node.ipfs_uri)
            .arg(&name)
            .arg("subgraph.yaml.patched")
            .current_dir(&dir.path);
        run_checked(cmd).await?;

        Ok(name)
    }

    /// Wait until the subgraph has synced or failed
    pub async fn wait_ready(name: &str) -> anyhow::Result<Subgraph> {
        let start = Instant::now();
        while start.elapsed() <= CONFIG.timeout {
            if let Some(subgraph) = Self::status(&name).await? {
                if subgraph.synced || !subgraph.healthy {
                    return Ok(subgraph);
                }
            }
            sleep(Duration::from_millis(2000)).await;
        }
        Err(anyhow!("Subgraph {} never synced or failed", name))
    }

    pub async fn status(name: &str) -> anyhow::Result<Option<Subgraph>> {
        #[derive(Deserialize)]
        struct Status {
            pub subgraph: String,
            pub health: String,
            pub synced: bool,
        }

        let query = format!(
            r#"query {{ status: indexingStatusesForSubgraphName(subgraphName: "{}") {{
              subgraph health synced
            }} }}"#,
            name
        );
        let body = graphql_query(&CONFIG.graph_node.index_node_uri(), &query).await?;
        let status = &body["data"]["status"];
        if status.is_null() || status.as_array().unwrap().is_empty() {
            return Ok(None);
        }
        let status: Status = serde_json::from_value(status[0].clone())?;
        let subgraph = Subgraph {
            name: name.to_string(),
            deployment: status.subgraph,
            synced: status.synced,
            healthy: status.health == "healthy",
        };
        Ok(Some(subgraph))
    }

    /// Make a GraphQL query to the subgraph's data API
    pub async fn query(&self, text: &str) -> anyhow::Result<Value> {
        let endpoint = format!(
            "{}/subgraphs/name/{}",
            CONFIG.graph_node.http_uri(),
            self.name
        );
        graphql_query(&endpoint, text).await
    }

    /// Make a GraphQL query to the index node API
    pub async fn index_with_vars(&self, text: &str, vars: Value) -> anyhow::Result<Value> {
        let endpoint = CONFIG.graph_node.index_node_uri();
        graphql_query_with_vars(&endpoint, text, vars).await
    }
}
