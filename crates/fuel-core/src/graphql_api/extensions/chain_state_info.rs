use std::sync::Arc;

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

use crate::graphql_api::{
    api_service::ConsensusProvider,
    database::ReadDatabase,
};

const CURRENT_STF_VERSION: &str = "current_stf_version";
const CURRENT_CONSENSUS_PARAMETERS_VERSION: &str = "current_consensus_parameters_version";

#[derive(Debug, derive_more::Display, derive_more::From)]
pub(crate) struct ChainStateInfoExtension;
impl ChainStateInfoExtension {
    pub fn new() -> Self {
        Self
    }
}

impl ExtensionFactory for ChainStateInfoExtension {
    fn create(&self) -> Arc<dyn Extension> {
        Arc::new(ChainStateInfoExtension::new())
    }
}

#[async_trait::async_trait]
impl Extension for ChainStateInfoExtension {
    async fn execute(
        &self,
        ctx: &ExtensionContext<'_>,
        operation_name: Option<&str>,
        next: NextExecute<'_>,
    ) -> Response {
        let db = ctx.data_unchecked::<ReadDatabase>();

        let mut response = next.run(ctx, operation_name).await;

        let consensus_parameters_provider = ctx.data_unchecked::<ConsensusProvider>();
        let current_consensus_parameters_version =
            consensus_parameters_provider.latest_consensus_parameters_version();
        response.extensions.insert(
            CURRENT_CONSENSUS_PARAMETERS_VERSION.to_string(),
            Value::Number(current_consensus_parameters_version.into()),
        );

        if let Ok(view) = db.view() {
            if let Ok(latest_block) = view.latest_block() {
                let current_stf_version =
                    latest_block.header().state_transition_bytecode_version();
                response.extensions.insert(
                    CURRENT_STF_VERSION.to_string(),
                    Value::Number(current_stf_version.into()),
                );
            }
        }

        response
    }
}
