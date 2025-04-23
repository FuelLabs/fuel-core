use std::{
    collections::BTreeMap,
    sync::Arc,
};

use async_graphql::{
    ErrorExtensionValues,
    Request,
    Response,
    ServerResult,
    Value,
    extensions::{
        Extension,
        ExtensionContext,
        ExtensionFactory,
        NextExecute,
        NextPrepareRequest,
    },
};
use fuel_core_types::fuel_types::BlockHeight;

use crate::graphql_api::{
    api_service::ChainInfoProvider,
    block_height_subscription,
};

pub(crate) const CURRENT_STF_VERSION: &str = "current_stf_version";
pub(crate) const CURRENT_CONSENSUS_PARAMETERS_VERSION: &str =
    "current_consensus_parameters_version";
pub(crate) const CURRENT_FUEL_BLOCK_HEIGHT: &str = "current_fuel_block_height";

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
    async fn prepare_request(
        &self,
        ctx: &ExtensionContext<'_>,
        request: Request,
        next: NextPrepareRequest<'_>,
    ) -> ServerResult<Request> {
        let result = next.run(ctx, request).await;

        match result {
            Ok(request) => Ok(request),
            Err(mut error) => {
                if error.extensions.is_none() {
                    error.extensions = Some(Default::default());
                }

                let extensions = error.extensions.as_mut().expect("It is set above; qed");
                let chain_state_info_provider = ctx.data_unchecked::<ChainInfoProvider>();
                set_current_state(
                    chain_state_info_provider,
                    extensions,
                    self.block_height_subscriber.current_block_height(),
                );

                Err(error)
            }
        }
    }

    async fn execute(
        &self,
        ctx: &ExtensionContext<'_>,
        operation_name: Option<&str>,
        next: NextExecute<'_>,
    ) -> Response {
        let mut response = next.run(ctx, operation_name).await;
        let chain_state_info_provider = ctx.data_unchecked::<ChainInfoProvider>();
        set_current_state(
            chain_state_info_provider,
            &mut response.extensions,
            self.block_height_subscriber.current_block_height(),
        );

        response
    }
}

fn set_current_state<E>(
    chain_state_info_provider: &ChainInfoProvider,
    extensions: &mut E,
    current_block_height: BlockHeight,
) where
    E: SetExtensionsResponse,
{
    let current_consensus_parameters_version =
        chain_state_info_provider.current_consensus_parameters_version();
    extensions.set(
        CURRENT_CONSENSUS_PARAMETERS_VERSION,
        Value::Number(current_consensus_parameters_version.into()),
    );

    let current_stf_version = chain_state_info_provider.current_stf_version();
    extensions.set(
        CURRENT_STF_VERSION,
        Value::Number(current_stf_version.into()),
    );

    let current_block_height: u32 = *current_block_height;
    extensions.set(
        CURRENT_FUEL_BLOCK_HEIGHT,
        Value::Number(current_block_height.into()),
    );
}

trait SetExtensionsResponse {
    fn set(&mut self, name: impl AsRef<str>, value: impl Into<Value>);
}

impl SetExtensionsResponse for ErrorExtensionValues {
    fn set(&mut self, name: impl AsRef<str>, value: impl Into<Value>) {
        self.set(name, value)
    }
}

impl SetExtensionsResponse for BTreeMap<String, Value> {
    fn set(&mut self, name: impl AsRef<str>, value: impl Into<Value>) {
        self.insert(name.as_ref().to_string(), value.into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graphql_api::extensions::unify_response;
    use async_graphql::{
        Response,
        ServerError,
    };
    use fuel_core_types::blockchain::header::{
        ConsensusParametersVersion,
        StateTransitionBytecodeVersion,
    };

    #[test]
    fn unify_response_method_is_updated() {
        use crate::graphql_api::ports::MockChainStateProvider;

        const STV_VERSION: StateTransitionBytecodeVersion = 12;
        const CPV_VERSION: ConsensusParametersVersion = 99;
        const BLOCK_HEIGHT: u32 = 100;

        let mut mocked_provider = MockChainStateProvider::default();
        mocked_provider
            .expect_current_consensus_parameters_version()
            .return_const(CPV_VERSION);
        mocked_provider
            .expect_current_stf_version()
            .return_const(STV_VERSION);

        let mocked_provider: ChainInfoProvider = Box::new(mocked_provider);

        // Given
        let mut response = Response::default();
        set_current_state(
            &mocked_provider,
            &mut response.extensions,
            BLOCK_HEIGHT.into(),
        );

        let mut error = ServerError::new("Error", None);
        error.extensions = Some(Default::default());
        let error_extensions = error.extensions.as_mut().unwrap();
        set_current_state(&mocked_provider, error_extensions, BLOCK_HEIGHT.into());
        let error_response = Response::from_errors(vec![error]);

        // When
        let mut unified_error_response = unify_response(error_response);

        // Then
        unified_error_response.errors.clear();

        assert_eq!(response, unified_error_response);
    }
}
