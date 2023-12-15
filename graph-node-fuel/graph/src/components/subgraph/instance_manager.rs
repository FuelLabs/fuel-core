use crate::prelude::BlockNumber;
use std::sync::Arc;

use crate::components::store::DeploymentLocator;

/// A `SubgraphInstanceManager` loads and manages subgraph instances.
///
/// When a subgraph is added, the subgraph instance manager creates and starts
/// a subgraph instances for the subgraph. When a subgraph is removed, the
/// subgraph instance manager stops and removes the corresponding instance.
#[async_trait::async_trait]
pub trait SubgraphInstanceManager: Send + Sync + 'static {
    async fn start_subgraph(
        self: Arc<Self>,
        deployment: DeploymentLocator,
        manifest: serde_yaml::Mapping,
        stop_block: Option<BlockNumber>,
    );
    async fn stop_subgraph(&self, deployment: DeploymentLocator);
}
