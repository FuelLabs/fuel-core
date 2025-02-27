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
    api_service::ChainInfoProvider,
    block_height_subscription,
};

const CURRENT_STF_VERSION: &str = "current_stf_version";
const CURRENT_CONSENSUS_PARAMETERS_VERSION: &str = "current_consensus_parameters_version";
const CURRENT_FUEL_BLOCK_HEIGHT: &str = "current_fuel_block_height";

#[derive(Debug)]
pub(crate) struct ChainStateInfoExtension {
    block_height_subscriber: block_height_subscription::Subscriber,
}

impl ChainStateInfoExtension {
    pub fn new(block_height_subscriber: block_height_subscription::Subscriber) -> Self {
        Self {
            block_height_subscriber,
        }
    }
}

impl ExtensionFactory for ChainStateInfoExtension {
    fn create(&self) -> Arc<dyn Extension> {
        Arc::new(ChainStateInfoExtension::new(
            self.block_height_subscriber.clone(),
        ))
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
        let mut response = next.run(ctx, operation_name).await;

        let chain_state_info_provider = ctx.data_unchecked::<ChainInfoProvider>();
        let current_consensus_parameters_version =
            chain_state_info_provider.current_consensus_parameters_version();
        response.extensions.insert(
            CURRENT_CONSENSUS_PARAMETERS_VERSION.to_string(),
            Value::Number(current_consensus_parameters_version.into()),
        );

        let current_stf_version = chain_state_info_provider.current_stf_version();
        response.extensions.insert(
            CURRENT_STF_VERSION.to_string(),
            Value::Number(current_stf_version.into()),
        );

        let current_block_height = self.block_height_subscriber.current_block_height();
        let current_block_height: u32 = *current_block_height;
        response.extensions.insert(
            CURRENT_FUEL_BLOCK_HEIGHT.to_string(),
            Value::Number(current_block_height.into()),
        );

        response
    }
}
