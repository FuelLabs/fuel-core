use async_graphql::{
    extensions::{
        Extension,
        ExtensionContext,
        ExtensionFactory,
        NextRequest,
        NextResolve,
        ResolveInfo,
    },
    QueryPathSegment,
    Response,
    ServerResult,
    Value,
};
use fuel_core_metrics::graphql_metrics::GRAPHQL_METRICS;
use std::sync::Arc;
use tokio::time::Instant;

pub(crate) struct PrometheusExtension {}

impl ExtensionFactory for PrometheusExtension {
    fn create(&self) -> Arc<dyn Extension> {
        Arc::new(PrometheusExtInner {})
    }
}

pub(crate) struct PrometheusExtInner;

#[async_trait::async_trait]
impl Extension for PrometheusExtInner {
    async fn request(
        &self,
        ctx: &ExtensionContext<'_>,
        next: NextRequest<'_>,
    ) -> Response {
        let start_time = Instant::now();
        let result = next.run(ctx).await;
        let seconds = start_time.elapsed().as_secs_f64();
        GRAPHQL_METRICS.graphql_observe("request", seconds);

        result
    }

    async fn resolve(
        &self,
        ctx: &ExtensionContext<'_>,
        info: ResolveInfo<'_>,
        next: NextResolve<'_>,
    ) -> ServerResult<Option<Value>> {
        let field_name = match (info.path_node.parent, info.path_node.segment) {
            (None, QueryPathSegment::Name(field_name)) => Some(field_name),
            _ => None,
        };

        let start_time = Instant::now();
        let res = next.run(ctx, info).await;
        let seconds = start_time.elapsed().as_secs_f64();

        if let Some(field_name) = field_name {
            GRAPHQL_METRICS.graphql_observe(field_name, seconds);
        }

        res
    }
}
