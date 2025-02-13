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

use crate::graphql_api::database::ReadDatabase;

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
        let db = ctx.data_unchecked::<ReadDatabase>();

        let mut response = next.run(ctx, operation_name).await;
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
