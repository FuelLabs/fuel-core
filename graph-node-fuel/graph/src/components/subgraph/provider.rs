use async_trait::async_trait;

use crate::{components::store::DeploymentLocator, prelude::*};

/// Common trait for subgraph providers.
#[async_trait]
pub trait SubgraphAssignmentProvider: Send + Sync + 'static {
    async fn start(
        &self,
        deployment: DeploymentLocator,
        stop_block: Option<BlockNumber>,
    ) -> Result<(), SubgraphAssignmentProviderError>;
    async fn stop(
        &self,
        deployment: DeploymentLocator,
    ) -> Result<(), SubgraphAssignmentProviderError>;
}
