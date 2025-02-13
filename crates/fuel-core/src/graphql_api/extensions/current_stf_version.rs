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
        let current_stf_version = 2137;

        let mut response = next.run(ctx, operation_name).await;
        response.extensions.insert(
            CURRENT_STF_VERSION.to_string(),
            Value::Number(current_stf_version.into()),
        );
        response
    }
}
