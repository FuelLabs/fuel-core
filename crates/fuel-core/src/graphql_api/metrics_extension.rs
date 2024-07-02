use async_graphql::{
    extensions::{
        Extension,
        ExtensionContext,
        ExtensionFactory,
        NextParseQuery,
        NextRequest,
        NextResolve,
        ResolveInfo,
    },
    parser::types::ExecutableDocument,
    Response,
    ServerResult,
    Value,
    Variables,
};
use fuel_core_metrics::graphql_metrics::graphql_metrics;
use std::{
    sync::{
        Arc,
        OnceLock,
    },
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
            current_query: OnceLock::new(),
        })
    }
}

pub(crate) struct MetricsExtInner {
    log_threshold_ms: Duration,
    current_query: OnceLock<String>,
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
        graphql_metrics().graphql_observe("request", seconds);

        result
    }

    async fn parse_query(
        &self,
        ctx: &ExtensionContext<'_>,
        query: &str,
        variables: &Variables,
        next: NextParseQuery<'_>,
    ) -> ServerResult<ExecutableDocument> {
        let doc = next.run(ctx, query, variables).await?;
        let set_query_res = self.current_query.set(query.to_string());
        if set_query_res.is_err() {
            tracing::warn!("Failed to save current query {query:?}");
        }
        Ok(doc)
    }

    async fn resolve(
        &self,
        ctx: &ExtensionContext<'_>,
        info: ResolveInfo<'_>,
        next: NextResolve<'_>,
    ) -> ServerResult<Option<Value>> {
        let field_name = match (info.path_node.parent, info.name) {
            (None, field_name) => Some(field_name),
            _ => None,
        };

        let start_time = Instant::now();
        let res = next.run(ctx, info).await;
        let elapsed = start_time.elapsed();

        if let Some(field_name) = field_name {
            graphql_metrics().graphql_observe(field_name, elapsed.as_secs_f64());
        }

        if elapsed > self.log_threshold_ms {
            let query = self
                .current_query
                .get()
                .map(String::as_str)
                .unwrap_or("UNKNOWN");
            tracing::info!(
                "Query {:?} exceeded threshold of {:?} seconds at {:?} seconds",
                query,
                self.log_threshold_ms.as_secs_f64(),
                elapsed.as_secs_f64()
            );
        }

        res
    }
}
