use crate::graphql_api::database::ReadDatabase;
use async_graphql::{
    extensions::{
        Extension,
        ExtensionContext,
        ExtensionFactory,
        NextPrepareRequest,
    },
    Request,
    ServerResult,
};
use std::sync::Arc;

/// The extension that adds the `ReadView` to the request context.
/// It guarantees that the request works with the one view of the database,
/// and external database modification cannot affect the result.
pub(crate) struct ViewExtension;

impl ViewExtension {
    pub fn new() -> Self {
        Self
    }
}

impl ExtensionFactory for ViewExtension {
    fn create(&self) -> Arc<dyn Extension> {
        Arc::new(ViewExtension::new())
    }
}

#[async_trait::async_trait]
impl Extension for ViewExtension {
    async fn prepare_request(
        &self,
        ctx: &ExtensionContext<'_>,
        request: Request,
        next: NextPrepareRequest<'_>,
    ) -> ServerResult<Request> {
        let database: &ReadDatabase = ctx.data_unchecked();
        let view = database.view();
        let request = request.data(view);
        next.run(ctx, request).await
    }
}
