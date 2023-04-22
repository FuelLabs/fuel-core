use async_graphql::{
    extensions::{
        Extension,
        ExtensionContext,
        ExtensionFactory,
        NextRequest,
        NextResolve,
        ResolveInfo,
    },
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
        let field_name = info.path_node.field_name();
        let path = info.path_node.to_string();

        let start_time = Instant::now();
        let res = next.run(ctx, info).await;
        let seconds = start_time.elapsed().as_secs_f64();
        GRAPHQL_METRICS.graphql_observe(field_name, seconds);
        GRAPHQL_METRICS.graphql_observe(path.as_str(), seconds);

        res
    }
}
