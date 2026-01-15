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
            expensive_root_field_name: OnceLock::new(),
        })
    }
}

pub struct ExpensiveOpGuard {
    expensive_root_field_names: Arc<[String]>,
    semaphore: Arc<Semaphore>,
    timeout: Duration,
    expensive_root_field_name: OnceLock<Option<String>>,
}

#[async_trait::async_trait]
impl Extension for ExpensiveOpGuard {
    async fn execute(
        &self,
        ctx: &ExtensionContext<'_>,
        operation_name: Option<&str>,
        next: NextExecute<'_>,
    ) -> Response {
        let expensive_root_field_name = self
            .expensive_root_field_name
            .get()
            .and_then(|name| name.as_ref());
        let is_expensive = expensive_root_field_name.is_some();

        tracing::warn!(
            "Executing operation: {:?}, expensive root field: {:?}, expected root fields: {:?}, expensive: {:?}, timeout: {:?}, semaphore_size: {:?}",
            operation_name,
            expensive_root_field_name,
            self.expensive_root_field_names,
            is_expensive,
            self.timeout,
            self.semaphore.available_permits(),
        );

        if !is_expensive {
            return next.run(ctx, operation_name).await;
        }

        // Concurrency gate (bulkhead)
        let permit = match self.semaphore.clone().try_acquire_owned() {
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

    async fn parse_query(
        &self,
        _ctx: &ExtensionContext<'_>,
        _query: &str,
        _variables: &Variables,
        next: NextParseQuery<'_>,
    ) -> ServerResult<ExecutableDocument> {
        let doc = next.run(_ctx, _query, _variables).await?;
        let expensive_root_field_name =
            has_expensive_root_field(&doc, &self.expensive_root_field_names);
        let _ = self
            .expensive_root_field_name
            .set(expensive_root_field_name);
        Ok(doc)
    }
}

fn has_expensive_root_field(
    doc: &ExecutableDocument,
    expensive_field_names: &[String],
) -> Option<String> {
    doc.operations.iter().find_map(|(_, op)| {
        selection_set_has_expensive_field(
            &op.node.selection_set,
            &doc.fragments,
            expensive_field_names,
        )
    })
}

fn selection_set_has_expensive_field(
    selection_set: &Positioned<SelectionSet>,
    fragments: &std::collections::HashMap<
        async_graphql::Name,
        Positioned<FragmentDefinition>,
    >,
    expensive_field_names: &[String],
) -> Option<String> {
    selection_set
        .node
        .items
        .iter()
        .find_map(|selection| match &selection.node {
            Selection::Field(field) => {
                let field_name = field.node.name.node.as_str();
                if expensive_field_names.iter().any(|name| name == field_name) {
                    Some(field_name.to_string())
                } else {
                    None
                }
            }
            Selection::FragmentSpread(spread) => fragments
                .get(&spread.node.fragment_name.node)
                .and_then(|fragment| {
                    selection_set_has_expensive_field(
                        &fragment.node.selection_set,
                        fragments,
                        expensive_field_names,
                    )
                }),
            Selection::InlineFragment(inline) => selection_set_has_expensive_field(
                &inline.node.selection_set,
                fragments,
                expensive_field_names,
            ),
        })
}
