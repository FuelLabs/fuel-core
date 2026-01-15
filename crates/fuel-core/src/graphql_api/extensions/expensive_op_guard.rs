use async_graphql::{
    Positioned,
    Response,
    ServerError,
    ServerResult,
    Variables,
    extensions::{
        Extension,
        ExtensionContext,
        ExtensionFactory,
        NextExecute,
        NextParseQuery,
    },
    parser::types::{
        ExecutableDocument,
        FragmentDefinition,
        Selection,
        SelectionSet,
    },
};
use std::{
    sync::{
        Arc,
        OnceLock,
    },
    time::Duration,
};
use tokio::sync::Semaphore;

pub struct ExpensiveOpGuardFactory {
    expensive_root_field_names: Arc<[String]>,
    semaphore: Arc<Semaphore>,
    timeout: Duration,
}

impl ExpensiveOpGuardFactory {
    pub fn new(
        expensive_root_field_names: Arc<[String]>,
        max_in_flight: usize,
        timeout: Duration,
    ) -> Self {
        Self {
            expensive_root_field_names,
            semaphore: Arc::new(Semaphore::new(max_in_flight)),
            timeout,
        }
    }
}

impl ExtensionFactory for ExpensiveOpGuardFactory {
    fn create(&self) -> Arc<dyn Extension> {
        Arc::new(ExpensiveOpGuard {
            expensive_root_field_names: self.expensive_root_field_names.clone(),
            semaphore: self.semaphore.clone(),
            timeout: self.timeout,
            expensive_root_field_count: OnceLock::new(),
        })
    }
}

pub struct ExpensiveOpGuard {
    expensive_root_field_names: Arc<[String]>,
    semaphore: Arc<Semaphore>,
    timeout: Duration,
    expensive_root_field_count: OnceLock<usize>,
}

#[async_trait::async_trait]
impl Extension for ExpensiveOpGuard {
    async fn parse_query(
        &self,
        _ctx: &ExtensionContext<'_>,
        _query: &str,
        _variables: &Variables,
        next: NextParseQuery<'_>,
    ) -> ServerResult<ExecutableDocument> {
        let doc = next.run(_ctx, _query, _variables).await?;
        let expensive_root_field_count =
            expensive_root_field_count(&doc, &self.expensive_root_field_names);
        let _ = self
            .expensive_root_field_count
            .set(expensive_root_field_count);
        Ok(doc)
    }

    async fn execute(
        &self,
        ctx: &ExtensionContext<'_>,
        operation_name: Option<&str>,
        next: NextExecute<'_>,
    ) -> Response {
        let expensive_root_field_count =
            self.expensive_root_field_count.get().copied().unwrap_or(0);
        let is_expensive = expensive_root_field_count > 0;

        tracing::debug!(
            "Executing operation: {:?}, expensive root field count: {:?}, expected root fields: {:?}, expensive: {:?}, timeout: {:?}, semaphore_size: {:?}",
            operation_name,
            expensive_root_field_count,
            self.expensive_root_field_names,
            is_expensive,
            self.timeout,
            self.semaphore.available_permits(),
        );

        match is_expensive {
            true => {
                self.expensive_execution(
                    ctx,
                    operation_name,
                    next,
                    expensive_root_field_count,
                )
                .await
            }
            false => next.run(ctx, operation_name).await,
        }
    }
}
impl ExpensiveOpGuard {
    async fn expensive_execution(
        &self,
        ctx: &ExtensionContext<'_>,
        operation_name: Option<&str>,
        next: NextExecute<'_>,
        expensive_root_field_count: usize,
    ) -> Response {
        let permit_count = match u32::try_from(expensive_root_field_count) {
            Ok(count) => count,
            Err(_) => u32::MAX,
        };

        // Concurrency gate (bulkhead)
        let permit = match self
            .semaphore
            .clone()
            .try_acquire_many_owned(permit_count)
        {
            Ok(p) => p,
            Err(_) => {
                let mut resp = Response::new(async_graphql::Value::Null);
                resp.errors.push(ServerError::new(
                    "Rate limit exceeded for this operation",
                    None,
                ));
                return resp;
            }
        };

        // Time bound (avoid request pile-ups)
        let fut = next.run(ctx, operation_name);
        let starting_time = tokio::time::Instant::now();
        let out = tokio::time::timeout(self.timeout, fut).await;
        tracing::warn!(
            "finished executing in {:?}ns, success: {:?}",
            starting_time.elapsed().as_nanos(),
            out.is_ok(),
        );

        drop(permit);

        match out {
            Ok(resp) => resp,
            Err(_) => {
                let mut resp = Response::new(async_graphql::Value::Null);
                resp.errors
                    .push(ServerError::new("Operation timed out", None));
                resp
            }
        }
    }
}

fn expensive_root_field_count(
    doc: &ExecutableDocument,
    expensive_field_names: &[String],
) -> usize {
    doc.operations
        .iter()
        .map(|(_, op)| {
            count_expensive_root_fields(
                &op.node.selection_set,
                &doc.fragments,
                expensive_field_names,
            )
        })
        .sum()
}

fn count_expensive_root_fields(
    selection_set: &Positioned<SelectionSet>,
    fragments: &std::collections::HashMap<
        async_graphql::Name,
        Positioned<FragmentDefinition>,
    >,
    expensive_field_names: &[String],
) -> usize {
    selection_set
        .node
        .items
        .iter()
        .map(|selection| match &selection.node {
            Selection::Field(field) => {
                let field_name = field.node.name.node.as_str();
                if expensive_field_names.iter().any(|name| name == field_name) {
                    1
                } else {
                    0
                }
            }
            Selection::FragmentSpread(spread) => fragments
                .get(&spread.node.fragment_name.node)
                .map(|fragment| {
                    count_expensive_root_fields(
                        &fragment.node.selection_set,
                        fragments,
                        expensive_field_names,
                    )
                })
                .unwrap_or(0),
            Selection::InlineFragment(inline) => count_expensive_root_fields(
                &inline.node.selection_set,
                fragments,
                expensive_field_names,
            ),
        })
        .sum()
}
