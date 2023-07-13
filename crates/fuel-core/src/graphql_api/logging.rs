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
use std::{
    sync::Arc,
    time::Duration,
};
use tokio::time::Instant;

pub(crate) struct LoggingExtension {
    log_threshold_ms: Duration,
}

impl LoggingExtension {
    pub fn new(log_threshold_ms: Duration) -> Self {
        LoggingExtension { log_threshold_ms }
    }
}

impl ExtensionFactory for LoggingExtension {
    fn create(&self) -> Arc<dyn Extension> {
        Arc::new(LoggingExtInner {
            log_threshold_ms: self.log_threshold_ms,
        })
    }
}

pub(crate) struct LoggingExtInner {
    log_threshold_ms: Duration,
}

#[async_trait::async_trait]
impl Extension for LoggingExtInner {
    async fn request(
        &self,
        ctx: &ExtensionContext<'_>,
        next: NextRequest<'_>,
    ) -> Response {
        next.run(ctx).await
    }

    async fn resolve(
        &self,
        ctx: &ExtensionContext<'_>,
        info: ResolveInfo<'_>,
        next: NextResolve<'_>,
    ) -> ServerResult<Option<Value>> {
        let start_time = Instant::now();
        let result = next.run(ctx, info).await;
        let elapsed = start_time.elapsed();

        if elapsed > self.log_threshold_ms {
            // TODO: Add more details to log?
            tracing::info!(
                "GraphQL query exceeded threshold of {:?} seconds at {:?} seconds",
                self.log_threshold_ms.as_secs_f64(),
                elapsed.as_secs_f64()
            );
        }

        result
    }
}
