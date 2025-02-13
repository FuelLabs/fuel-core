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

const CURRENT_STF_VERSION: &str = "current_stf_version";

/// The extension to attach the current STF version to all responses.
#[derive(Debug, derive_more::Display, derive_more::From)]
pub(crate) struct CurrentStfVersionExtension;
impl CurrentStfVersionExtension {
    pub fn new() -> Self {
        Self
    }
}

impl ExtensionFactory for CurrentStfVersionExtension {
    fn create(&self) -> Arc<dyn Extension> {
        Arc::new(CurrentStfVersionExtension::new())
    }
}

#[async_trait::async_trait]
impl Extension for CurrentStfVersionExtension {
    async fn execute(
        &self,
        ctx: &ExtensionContext<'_>,
        operation_name: Option<&str>,
        next: NextExecute<'_>,
    ) -> Response {
        // TODO[RC]: Obtain correct value.

        // - state_transition_bytecode_version lives in the BlockProducer
        // - all versions are stored in StateTransitionBytecodeVersions DB
        // - can be obtained by calling latest_state_transition_bytecode_version()
        //      - but this goes to DB, which can be slow
        // - the value is updated in the following places:
        //      - fn process_upgrade_transaction() - with fraud proofs enabled only
        //      - fn execute_genesis_block()
        //      - fn set_state_transition_bytecode() in vm_storage
        //
        // Possible ways to go:
        // 1) the way to go is to notify the extension about the changes and the extension will
        //    store the current version internally
        // 2) store the latest STF version in the `SharedState` - similarly to the latest consensus parameters
        // 3) just read from DB every time, relying on fast subsequent reads from RocksDB - see the conversation
        //    here: https://github.com/FuelLabs/fuel-core/pull/2463#discussion_r1882728054

        let current_stf_version = 2137;

        let mut response = next.run(ctx, operation_name).await;
        response.extensions.insert(
            CURRENT_STF_VERSION.to_string(),
            Value::Number(current_stf_version.into()),
        );
        response
    }
}
