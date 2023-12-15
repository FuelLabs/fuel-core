use std::str::FromStr;

use async_trait::async_trait;

use crate::{components::store::DeploymentLocator, prelude::*};

#[derive(Clone, Copy, Debug)]
pub enum SubgraphVersionSwitchingMode {
    Instant,
    Synced,
}

impl SubgraphVersionSwitchingMode {
    pub fn parse(mode: &str) -> Self {
        Self::from_str(mode).unwrap()
    }
}

impl FromStr for SubgraphVersionSwitchingMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "instant" => Ok(SubgraphVersionSwitchingMode::Instant),
            "synced" => Ok(SubgraphVersionSwitchingMode::Synced),
            _ => Err(format!("invalid version switching mode: {:?}", s)),
        }
    }
}

/// Common trait for subgraph registrars.
#[async_trait]
pub trait SubgraphRegistrar: Send + Sync + 'static {
    async fn create_subgraph(
        &self,
        name: SubgraphName,
    ) -> Result<CreateSubgraphResult, SubgraphRegistrarError>;

    async fn create_subgraph_version(
        &self,
        name: SubgraphName,
        hash: DeploymentHash,
        assignment_node_id: NodeId,
        debug_fork: Option<DeploymentHash>,
        start_block_block: Option<BlockPtr>,
        graft_block_override: Option<BlockPtr>,
        history_blocks: Option<i32>,
    ) -> Result<DeploymentLocator, SubgraphRegistrarError>;

    async fn remove_subgraph(&self, name: SubgraphName) -> Result<(), SubgraphRegistrarError>;

    async fn reassign_subgraph(
        &self,
        hash: &DeploymentHash,
        node_id: &NodeId,
    ) -> Result<(), SubgraphRegistrarError>;
}
