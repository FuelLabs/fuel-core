use std::collections::HashSet;
use std::fmt;
use std::str::FromStr;

use diesel::{dsl::sql, prelude::*};
use diesel::{sql_types::Text, PgConnection};

use graph::components::store::DeploymentId;
use graph::{
    components::store::DeploymentLocator,
    data::subgraph::status,
    prelude::{anyhow, lazy_static, regex::Regex, DeploymentHash},
};
use graph_store_postgres::command_support::catalog as store_catalog;
use graph_store_postgres::connection_pool::ConnectionPool;

use crate::manager::display::List;

lazy_static! {
    // `Qm...` optionally follow by `:$shard`
    static ref HASH_RE: Regex = Regex::new("\\A(?P<hash>Qm[^:]+)(:(?P<shard>[a-z0-9_]+))?\\z").unwrap();
    // `sgdNNN`
    static ref DEPLOYMENT_RE: Regex = Regex::new("\\A(?P<nsp>sgd[0-9]+)\\z").unwrap();
}

/// A search for one or multiple deployments to make it possible to search
/// by subgraph name, IPFS hash, or namespace. Since there can be multiple
/// deployments for the same IPFS hash, the search term for a hash can
/// optionally specify a shard.
#[derive(Clone, Debug)]
pub enum DeploymentSearch {
    Name { name: String },
    Hash { hash: String, shard: Option<String> },
    All,
    Deployment { namespace: String },
}

impl fmt::Display for DeploymentSearch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeploymentSearch::Name { name } => write!(f, "{}", name),
            DeploymentSearch::Hash {
                hash,
                shard: Some(shard),
            } => write!(f, "{}:{}", hash, shard),
            DeploymentSearch::All => Ok(()),
            DeploymentSearch::Hash { hash, shard: None } => write!(f, "{}", hash),
            DeploymentSearch::Deployment { namespace } => write!(f, "{}", namespace),
        }
    }
}

impl FromStr for DeploymentSearch {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Some(caps) = HASH_RE.captures(s) {
            let hash = caps.name("hash").unwrap().as_str().to_string();
            let shard = caps.name("shard").map(|shard| shard.as_str().to_string());
            Ok(DeploymentSearch::Hash { hash, shard })
        } else if let Some(caps) = DEPLOYMENT_RE.captures(s) {
            let namespace = caps.name("nsp").unwrap().as_str().to_string();
            Ok(DeploymentSearch::Deployment { namespace })
        } else {
            Ok(DeploymentSearch::Name {
                name: s.to_string(),
            })
        }
    }
}

impl DeploymentSearch {
    pub fn lookup(&self, primary: &ConnectionPool) -> Result<Vec<Deployment>, anyhow::Error> {
        let conn = primary.get()?;
        self.lookup_with_conn(&conn)
    }

    pub fn lookup_with_conn(&self, conn: &PgConnection) -> Result<Vec<Deployment>, anyhow::Error> {
        use store_catalog::deployment_schemas as ds;
        use store_catalog::subgraph as s;
        use store_catalog::subgraph_deployment_assignment as a;
        use store_catalog::subgraph_version as v;

        let query = ds::table
            .inner_join(v::table.on(v::deployment.eq(ds::subgraph)))
            .inner_join(s::table.on(v::subgraph.eq(s::id)))
            .left_outer_join(a::table.on(a::id.eq(ds::id)))
            .select((
                s::name,
                sql::<Text>(
                    "(case
                    when subgraphs.subgraph.pending_version = subgraphs.subgraph_version.id then 'pending'
                    when subgraphs.subgraph.current_version = subgraphs.subgraph_version.id then 'current'
                    else 'unused' end) status",
                ),
                v::deployment,
                ds::name,
                ds::id,
                a::node_id.nullable(),
                ds::shard,
                ds::network,
                ds::active,
            ));

        let deployments: Vec<Deployment> = match self {
            DeploymentSearch::Name { name } => {
                let pattern = format!("%{}%", name);
                query.filter(s::name.ilike(&pattern)).load(conn)?
            }
            DeploymentSearch::Hash { hash, shard } => {
                let query = query.filter(ds::subgraph.eq(&hash));
                match shard {
                    Some(shard) => query.filter(ds::shard.eq(shard)).load(conn)?,
                    None => query.load(conn)?,
                }
            }
            DeploymentSearch::Deployment { namespace } => {
                query.filter(ds::name.eq(&namespace)).load(conn)?
            }
            DeploymentSearch::All => query.load(conn)?,
        };
        Ok(deployments)
    }

    /// Finds all [`Deployment`]s for this [`DeploymentSearch`].
    pub fn find(
        &self,
        pool: ConnectionPool,
        current: bool,
        pending: bool,
        used: bool,
    ) -> Result<Vec<Deployment>, anyhow::Error> {
        let current = current || used;
        let pending = pending || used;

        let deployments = self.lookup(&pool)?;
        // Filter by status; if neither `current` or `pending` are set, list
        // all deployments
        let deployments: Vec<_> = deployments
            .into_iter()
            .filter(|deployment| match (current, pending) {
                (true, false) => deployment.status == "current",
                (false, true) => deployment.status == "pending",
                (true, true) => deployment.status == "current" || deployment.status == "pending",
                (false, false) => true,
            })
            .collect();
        Ok(deployments)
    }

    /// Finds a single deployment locator for the given deployment identifier.
    pub fn locate_unique(&self, pool: &ConnectionPool) -> anyhow::Result<DeploymentLocator> {
        let mut locators: Vec<DeploymentLocator> = HashSet::<DeploymentLocator>::from_iter(
            self.lookup(pool)?
                .into_iter()
                .map(|deployment| deployment.locator()),
        )
        .into_iter()
        .collect();
        let deployment_locator = match locators.len() {
            0 => anyhow::bail!("Found no deployment for `{}`", self),
            1 => locators.pop().unwrap(),
            n => anyhow::bail!("Found {} deployments for `{}`", n, self),
        };
        Ok(deployment_locator)
    }
}

#[derive(Queryable, PartialEq, Eq, Hash, Debug)]
pub struct Deployment {
    pub name: String,
    pub status: String,
    pub deployment: String,
    pub namespace: String,
    pub id: i32,
    pub node_id: Option<String>,
    pub shard: String,
    pub chain: String,
    pub active: bool,
}

impl Deployment {
    pub fn locator(&self) -> DeploymentLocator {
        DeploymentLocator::new(
            DeploymentId(self.id),
            DeploymentHash::new(self.deployment.clone()).unwrap(),
        )
    }

    pub fn print_table(deployments: Vec<Self>, statuses: Vec<status::Info>) {
        let mut rows = vec![
            "name",
            "status",
            "id",
            "namespace",
            "shard",
            "active",
            "chain",
            "node_id",
        ];
        if !statuses.is_empty() {
            rows.extend(vec![
                "paused",
                "synced",
                "health",
                "earliest block",
                "latest block",
                "chain head block",
            ]);
        }

        let mut list = List::new(rows);

        for deployment in deployments {
            let status = statuses
                .iter()
                .find(|status| &status.id.0 == &deployment.id);

            let mut rows = vec![
                deployment.name,
                deployment.status,
                deployment.deployment,
                deployment.namespace,
                deployment.shard,
                deployment.active.to_string(),
                deployment.chain,
                deployment.node_id.unwrap_or("---".to_string()),
            ];
            if let Some(status) = status {
                let chain = &status.chains[0];
                rows.extend(vec![
                    status
                        .paused
                        .map(|b| b.to_string())
                        .unwrap_or("---".to_string()),
                    status.synced.to_string(),
                    status.health.as_str().to_string(),
                    chain.earliest_block_number.to_string(),
                    chain
                        .latest_block
                        .as_ref()
                        .map(|b| b.number().to_string())
                        .unwrap_or("-".to_string()),
                    chain
                        .chain_head_block
                        .as_ref()
                        .map(|b| b.number().to_string())
                        .unwrap_or("-".to_string()),
                ])
            }
            list.append(rows);
        }

        list.render();
    }
}
