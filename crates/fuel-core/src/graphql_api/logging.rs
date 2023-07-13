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
            tracing::info!(
                "GraphQL query exceeded threshold of {:?} seconds at {:?} seconds",
                self.log_threshold_ms.as_secs_f64(),
                elapsed.as_secs_f64()
            );
            match &result {
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

        result
    }

    // async fn parse_query(
    //     &self,
    //     ctx: &ExtensionContext<'_>,
    //     query: &str,
    //     variables: &Variables,
    //     next: NextParseQuery<'_>,
    // ) -> ServerResult<ExecutableDocument> {
    //     let start_time = Instant::now();
    //     let result = next.run(ctx, query, variables).await;
    //     let elapsed = start_time.elapsed();
    //
    //     tracing::info!("parse query");
    //
    //     if elapsed > self.log_threshold_ms {
    //         tracing::info!(
    //             "GraphQL query exceeded threshold of {:?} seconds at {:?} seconds",
    //             self.log_threshold_ms.as_secs_f64(),
    //             elapsed.as_secs_f64()
    //         );
    //         tracing::info!("GraphQL query parse result: {:?}", &result);
    //     }
    //
    //     result
    // }
}
