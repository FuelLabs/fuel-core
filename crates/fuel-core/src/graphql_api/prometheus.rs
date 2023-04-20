use async_graphql::{
    extensions::{
        Extension,
        ExtensionContext,
        ExtensionFactory,
        NextParseQuery,
        NextRequest,
    },
    parser::types::{
        ExecutableDocument,
        OperationType,
        Selection,
    },
    Response,
    ServerResult,
    Variables,
};
use fuel_core_metrics::graphql_metrics::GRAPHQL_METRICS;
use std::{
    sync::Arc,
    time::Instant,
};
use tokio::sync::RwLock;

pub(crate) struct PrometheusExtension {}

impl ExtensionFactory for PrometheusExtension {
    fn create(&self) -> Arc<dyn Extension> {
        Arc::new(PrometheusExtInner {
            operation_name: RwLock::new(None),
        })
    }
}

pub(crate) struct PrometheusExtInner {
    operation_name: RwLock<Option<String>>,
}

#[async_trait::async_trait]
impl Extension for PrometheusExtInner {
    async fn request(
        &self,
        ctx: &ExtensionContext<'_>,
        next: NextRequest<'_>,
    ) -> Response {
        let start_time = Instant::now();
        let result = next.run(ctx).await;

        let op_name = self.operation_name.read().await;
        if let Some(op) = &*op_name {
            GRAPHQL_METRICS.graphql_observe(op, start_time.elapsed().as_secs_f64());
        }

        result
    }

    async fn parse_query(
        &self,
        ctx: &ExtensionContext<'_>,
        query: &str,
        variables: &Variables,
        next: NextParseQuery<'_>,
    ) -> ServerResult<ExecutableDocument> {
        let document = next.run(ctx, query, variables).await?;
        let is_schema = document
            .operations
            .iter()
            .filter(|(_, operation)| operation.node.ty == OperationType::Query)
            .any(|(_, operation)| operation.node.selection_set.node.items.iter().any(|selection| matches!(&selection.node, Selection::Field(field) if field.node.name.node == "__schema")));
        if !is_schema {
            if let Some((_, def)) = document.operations.iter().next() {
                if let Some(Selection::Field(e)) =
                    &def.node.selection_set.node.items.get(0).map(|n| &n.node)
                {
                    // only track query if there's a single selection set
                    if def.node.selection_set.node.items.len() == 1 {
                        *self.operation_name.write().await =
                            Some(e.node.name.node.to_string());
                    }
                }
            }
        }
        Ok(document)
    }
}
