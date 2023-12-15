use graph::prelude::{BlockPtr, CheapClone, QueryExecutionError, QueryResult};
use std::sync::Arc;
use std::time::Instant;

use crate::execution::{ast as a, *};

/// Utilities for working with GraphQL query ASTs.
pub mod ast;

/// Extension traits
pub mod ext;

/// Options available for query execution.
pub struct QueryExecutionOptions<R> {
    /// The resolver to use.
    pub resolver: R,

    /// Time at which the query times out.
    pub deadline: Option<Instant>,

    /// Maximum value for the `first` argument.
    pub max_first: u32,

    /// Maximum value for the `skip` argument
    pub max_skip: u32,

    /// Whether to include an execution trace in the result
    pub trace: bool,
}

/// Executes a query and returns a result.
/// If the query is not cacheable, the `Arc` may be unwrapped.
pub async fn execute_query<R>(
    query: Arc<Query>,
    selection_set: Option<a::SelectionSet>,
    block_ptr: Option<BlockPtr>,
    options: QueryExecutionOptions<R>,
) -> Arc<QueryResult>
where
    R: Resolver,
{
    // Create a fresh execution context
    let ctx = Arc::new(ExecutionContext {
        logger: query.logger.clone(),
        resolver: options.resolver,
        query: query.clone(),
        deadline: options.deadline,
        max_first: options.max_first,
        max_skip: options.max_skip,
        cache_status: Default::default(),
        trace: options.trace,
    });

    if !query.is_query() {
        return Arc::new(
            QueryExecutionError::NotSupported("Only queries are supported".to_string()).into(),
        );
    }
    let selection_set = selection_set
        .map(Arc::new)
        .unwrap_or_else(|| query.selection_set.cheap_clone());

    // Execute top-level `query { ... }` and `{ ... }` expressions.
    let query_type = ctx.query.schema.query_type.cheap_clone().into();
    let start = Instant::now();
    let result = execute_root_selection_set(
        ctx.cheap_clone(),
        selection_set.cheap_clone(),
        query_type,
        block_ptr.clone(),
    )
    .await;
    let elapsed = start.elapsed();
    let cache_status = ctx.cache_status.load();
    ctx.resolver
        .record_work(query.as_ref(), elapsed, cache_status);
    query.log_cache_status(
        &selection_set,
        block_ptr.map(|b| b.number).unwrap_or(0),
        start,
        cache_status.to_string(),
    );
    result
}
