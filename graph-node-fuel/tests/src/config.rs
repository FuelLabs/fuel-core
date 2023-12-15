use std::time::{Duration, Instant};
use std::{fs, path::PathBuf};

use tokio::process::{Child, Command};

use graph::prelude::anyhow::Context;
use graph::prelude::diesel::connection::SimpleConnection;
use graph::prelude::{lazy_static, reqwest};
use tokio::time::sleep;

use crate::helpers::TestFile;
use crate::status;

lazy_static! {
    pub static ref CONFIG: Config = Config::default();
}

#[derive(Clone, Debug)]
pub struct DbConfig {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
    pub name: String,
}

impl DbConfig {
    pub fn url(&self) -> String {
        format!(
            "postgres://{}:{}@{}:{}/{}",
            self.user, self.password, self.host, self.port, self.name
        )
    }

    pub fn template_url(&self) -> String {
        format!(
            "postgres://{}:{}@{}:{}/{}",
            self.user, self.password, self.host, self.port, "template1"
        )
    }
}

#[derive(Clone, Debug)]
pub struct EthConfig {
    pub network: String,
    pub port: u16,
    pub host: String,
}

impl EthConfig {
    pub fn url(&self) -> String {
        format!("http://{}:{}", self.host, self.port)
    }

    pub fn network_url(&self) -> String {
        format!("{}:{}", self.network, self.url())
    }
}

#[derive(Clone, Debug)]
pub struct GraphNodePorts {
    pub http: u16,
    pub index: u16,
    pub ws: u16,
    pub admin: u16,
    pub metrics: u16,
}

impl Default for GraphNodePorts {
    fn default() -> Self {
        Self {
            http: 3030,
            ws: 3031,
            admin: 3032,
            index: 3033,
            metrics: 3034,
        }
    }
}

#[derive(Clone, Debug)]
pub struct GraphNodeConfig {
    bin: PathBuf,
    pub ports: GraphNodePorts,
    pub ipfs_uri: String,
    pub log_file: TestFile,
}

impl GraphNodeConfig {
    pub fn admin_uri(&self) -> String {
        format!("http://localhost:{}", self.ports.admin)
    }

    pub fn index_node_uri(&self) -> String {
        format!("http://localhost:{}/graphql", self.ports.index)
    }

    pub fn http_uri(&self) -> String {
        format!("http://localhost:{}", self.ports.http)
    }

    pub async fn check_if_up(&self) -> anyhow::Result<bool> {
        let url = format!("http://localhost:{}/", self.ports.http);
        let client = reqwest::Client::new();
        match client.get(&url).send().await {
            Ok(res) => {
                if res.status().is_success() {
                    Ok(true)
                } else {
                    Err(anyhow::anyhow!("got non-success for GET on {url}"))
                }
            }
            Err(e) => {
                if e.is_connect() {
                    Ok(false)
                } else {
                    Err(e).context(format!("failed to connect to {url}"))
                }
            }
        }
    }
}

impl Default for GraphNodeConfig {
    fn default() -> Self {
        let bin = fs::canonicalize("../target/debug/graph-node")
            .expect("failed to infer `graph-node` program location. (Was it built already?)");

        Self {
            bin,
            ports: GraphNodePorts::default(),
            ipfs_uri: "http://localhost:3001".to_string(),
            log_file: TestFile::new("integration-tests/graph-node.log"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Config {
    pub db: DbConfig,
    pub eth: EthConfig,
    pub graph_node: GraphNodeConfig,
    pub graph_cli: String,
    pub num_parallel_tests: usize,
    pub timeout: Duration,
}

impl Config {
    pub async fn spawn_graph_node(&self) -> anyhow::Result<Child> {
        let ports = &self.graph_node.ports;

        let args = [
            "--postgres-url",
            &self.db.url(),
            "--ethereum-rpc",
            &self.eth.network_url(),
            "--ipfs",
            &self.graph_node.ipfs_uri,
            "--http-port",
            &ports.http.to_string(),
            "--index-node-port",
            &ports.index.to_string(),
            "--ws-port",
            &ports.ws.to_string(),
            "--admin-port",
            &ports.admin.to_string(),
            "--metrics-port",
            &ports.metrics.to_string(),
        ];
        let stdout = self.graph_node.log_file.create();
        let stderr = stdout.try_clone()?;
        status!(
            "graph-node",
            "Writing logs to {}",
            self.graph_node.log_file.path.display()
        );
        let mut command = Command::new(self.graph_node.bin.as_os_str());
        command
            .stdout(stdout)
            .stderr(stderr)
            .args(args)
            .env("GRAPH_STORE_WRITE_BATCH_DURATION", "5");

        status!(
            "graph-node",
            "Starting: '{} {}'",
            self.graph_node.bin.as_os_str().to_string_lossy(),
            args.join(" ")
        );
        let child = command.spawn().context("failed to start graph-node")?;

        status!("graph-node", "Waiting to accept requests",);
        let start = Instant::now();
        loop {
            let up = self.graph_node.check_if_up().await?;
            if up {
                break;
            } else {
                sleep(Duration::from_millis(500)).await;
            }
        }
        status!("graph-node", "Up after {}ms", start.elapsed().as_millis());
        Ok(child)
    }

    pub fn reset_database(&self) {
        use graph::prelude::diesel::{Connection, PgConnection};
        // The drop and create statements need to happen as separate statements
        let drop_db = format!(r#"drop database if exists "{}""#, self.db.name);
        let create_db = format!(
            r#"create database "{}" template = 'template0' locale = 'C' encoding = 'UTF8'"#,
            self.db.name
        );
        let setup = format!(
            r#"
        create extension pg_trgm;
        create extension pg_stat_statements;
        create extension btree_gist;
        create extension postgres_fdw;
        grant usage on foreign data wrapper postgres_fdw to "{}";
        "#,
            self.db.user
        );

        let template_uri = self.db.template_url();

        let conn = PgConnection::establish(&template_uri)
            .expect("Failed to connect to template1 to reset database");
        conn.batch_execute(&drop_db)
            .expect("Failed to drop old database");
        conn.batch_execute(&create_db)
            .expect("Failed to create new database");

        let conn = PgConnection::establish(&self.db.url())
            .expect("Failed to connect to graph-node database");
        conn.batch_execute(&setup)
            .expect("Failed to reset Postgres database");
    }
}

impl Default for Config {
    fn default() -> Self {
        let graph_cli =
            std::env::var("GRAPH_CLI").unwrap_or_else(|_| "node_modules/.bin/graph".to_string());
        let num_parallel_tests = std::env::var("N_CONCURRENT_TESTS")
            .map(|x| x.parse().expect("N_CONCURRENT_TESTS must be a number"))
            .unwrap_or(1000);
        Config {
            db: DbConfig {
                host: "localhost".to_string(),
                port: 3011,
                user: "graph-node".to_string(),
                password: "let-me-in".to_string(),
                name: "graph-node".to_string(),
            },
            eth: EthConfig {
                network: "test".to_string(),
                port: 3021,
                host: "localhost".to_string(),
            },
            graph_node: GraphNodeConfig::default(),
            graph_cli,
            num_parallel_tests,
            timeout: Duration::from_secs(120),
        }
    }
}
