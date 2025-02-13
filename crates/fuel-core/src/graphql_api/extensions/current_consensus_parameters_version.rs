use std::sync::Arc;

use crate::graphql_api::api_service::ConsensusProvider;
use async_graphql::{
    extensions::{
        Extension,
        ExtensionContext,
        ExtensionFactory,
        NextExecute,
    },
    Response,
    Value,
};

const CURRENT_CONSENSUS_PARAMETERS_VERSION: &str = "current_consensus_parameters_version";

/// The extension to attach the current STF version to all responses.
#[derive(Debug, derive_more::Display, derive_more::From)]
pub(crate) struct CurrentConsensusParametersVersionExtension;
impl CurrentConsensusParametersVersionExtension {
    pub fn new() -> Self {
        Self
    }
}

impl ExtensionFactory for CurrentConsensusParametersVersionExtension {
    fn create(&self) -> Arc<dyn Extension> {
        Arc::new(CurrentConsensusParametersVersionExtension::new())
    }
}

#[async_trait::async_trait]
impl Extension for CurrentConsensusParametersVersionExtension {
    async fn execute(
        &self,
        ctx: &ExtensionContext<'_>,
        operation_name: Option<&str>,
        next: NextExecute<'_>,
    ) -> Response {
        let consensus_parameters_provider = ctx.data_unchecked::<ConsensusProvider>();

        let current_consensus_parameters_version =
            consensus_parameters_provider.latest_consensus_parameters_version();

        let mut response = next.run(ctx, operation_name).await;
        response.extensions.insert(
            CURRENT_CONSENSUS_PARAMETERS_VERSION.to_string(),
            Value::Number(current_consensus_parameters_version.into()),
        );
        response
    }
}
