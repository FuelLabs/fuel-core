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
use std::{
    sync::Arc,
    time::Duration,
};
use tokio::time::Instant;

pub(crate) struct MetricsExtension {
    log_threshold_ms: Duration,
}

impl MetricsExtension {
    pub fn new(log_threshold_ms: Duration) -> Self {
        MetricsExtension { log_threshold_ms }
    }
}

impl ExtensionFactory for MetricsExtension {
    fn create(&self) -> Arc<dyn Extension> {
        Arc::new(MetricsExtInner {
            log_threshold_ms: self.log_threshold_ms,
        })
    }
}

pub(crate) struct MetricsExtInner {
    log_threshold_ms: Duration,
}

#[async_trait::async_trait]
impl Extension for MetricsExtInner {
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
        let elapsed = start_time.elapsed();

        if let Some(field_name) = field_name {
            GRAPHQL_METRICS.graphql_observe(field_name, elapsed.as_secs_f64());
        }

        if elapsed > self.log_threshold_ms {
            tracing::info!(
                "GraphQL query exceeded threshold of {:?} seconds at {:?} seconds",
                self.log_threshold_ms.as_secs_f64(),
                elapsed.as_secs_f64()
            );
            match &res {
                Ok(inner) => match inner {
                    None => {
                        tracing::info!("Request: None");
                    }
                    Some(object) => {
                        tracing::info!("Request: {:?}", &object);
                    }
                },
                Err(err) => tracing::info!("GraphQL query resolve error: {:?}", &err),
            }
        }

        res
    }
}
