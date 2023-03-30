use async_graphql::{
    extensions::{
        Extension,
        ExtensionContext,
        ExtensionFactory,
        NextExecute,
        NextParseQuery,
        NextRequest,
        NextResolve,
        NextSubscribe,
        NextValidation,
        ResolveInfo,
        Tracing,
    },
    parser::types::ExecutableDocument,
    Response,
    ServerError,
    ServerResult,
    ValidationResult,
    Value,
    Variables,
};
use futures::stream::BoxStream;
use std::sync::Arc;
use tracing::instrument;
use tracing_honeycomb::{
    register_dist_tracing_root,
    TraceId,
};

pub(crate) struct HoneyTrace;

impl ExtensionFactory for HoneyTrace {
    fn create(&self) -> Arc<dyn Extension> {
        Arc::new(InternalHoneyTrace {
            inner_trace: Tracing::create(&Tracing),
        })
    }
}

struct InternalHoneyTrace {
    inner_trace: Arc<dyn Extension>,
}

#[async_trait::async_trait]
impl Extension for InternalHoneyTrace {
    #[instrument(skip(self, ctx, next))]
    async fn request(
        &self,
        ctx: &ExtensionContext<'_>,
        next: NextRequest<'_>,
    ) -> Response {
        let response = self.inner_trace.request(ctx, next);
        let _ = register_dist_tracing_root(TraceId::new(), None);
        response.await
    }

    #[instrument(skip(self, ctx, stream, next))]
    fn subscribe<'s>(
        &self,
        ctx: &ExtensionContext<'_>,
        stream: BoxStream<'s, Response>,
        next: NextSubscribe<'_>,
    ) -> BoxStream<'s, Response> {
        let response = self.inner_trace.subscribe(ctx, stream, next);
        let _ = register_dist_tracing_root(TraceId::new(), None);
        response
    }

    #[instrument(skip(self, ctx, query, variables, next))]
    async fn parse_query(
        &self,
        ctx: &ExtensionContext<'_>,
        query: &str,
        variables: &Variables,
        next: NextParseQuery<'_>,
    ) -> ServerResult<ExecutableDocument> {
        let response = self.inner_trace.parse_query(ctx, query, variables, next);
        let _ = register_dist_tracing_root(TraceId::new(), None);
        response.await
    }

    #[instrument(skip(self, ctx, next))]
    async fn validation(
        &self,
        ctx: &ExtensionContext<'_>,
        next: NextValidation<'_>,
    ) -> Result<ValidationResult, Vec<ServerError>> {
        let response = self.inner_trace.validation(ctx, next);
        let _ = register_dist_tracing_root(TraceId::new(), None);
        response.await
    }

    #[instrument(skip(self, ctx, operation_name, next))]
    async fn execute(
        &self,
        ctx: &ExtensionContext<'_>,
        operation_name: Option<&str>,
        next: NextExecute<'_>,
    ) -> Response {
        let response = self.inner_trace.execute(ctx, operation_name, next);
        let _ = register_dist_tracing_root(TraceId::new(), None);
        response.await
    }

    #[instrument(skip(self, ctx, info, next))]
    async fn resolve(
        &self,
        ctx: &ExtensionContext<'_>,
        info: ResolveInfo<'_>,
        next: NextResolve<'_>,
    ) -> ServerResult<Option<Value>> {
        let response = self.inner_trace.resolve(ctx, info, next);
        let _ = register_dist_tracing_root(TraceId::new(), None);
        response.await
    }
}
