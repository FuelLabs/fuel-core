use std::{
    sync::Arc,
    time::Instant,
};

use async_graphql::extensions::{
    Extension,
    ExtensionContext,
    ExtensionFactory,
    NextExecute,
    NextRequest,
};
use fuel_core_metrics::graphql_metrics::GRAPHQL_METRICS;

pub(crate) struct PrometheusExtension;

impl ExtensionFactory for PrometheusExtension {
    fn create(&self) -> Arc<dyn Extension> {
        Arc::new(PrometheusExtension)
    }
}

#[async_trait::async_trait]
impl Extension for PrometheusExtension {
    async fn request(
        &self,
        ctx: &ExtensionContext<'_>,
        next: NextRequest<'_>,
    ) -> async_graphql::Response {
        GRAPHQL_METRICS.num_of_requests.inc();

        next.run(ctx).await
    }

    async fn execute(
        &self,
        ctx: &ExtensionContext<'_>,
        operation_name: Option<&str>,
        next: NextExecute<'_>,
    ) -> async_graphql::Response {
        let start_time = Instant::now();

        let result = next.run(ctx, operation_name).await;

        GRAPHQL_METRICS
            .response_times
            .observe(start_time.elapsed().as_secs_f64());

        result
    }
}
