pub extern crate graphql_parser;

/// Utilities for schema introspection.
pub mod introspection;

/// Utilities for executing GraphQL.
mod execution;

/// Utilities for executing GraphQL queries and working with query ASTs.
pub mod query;

/// Utilities for executing GraphQL subscriptions.
pub mod subscription;

/// Utilities for working with GraphQL values.
mod values;

/// Utilities for querying `Store` components.
mod store;

/// The external interface for actually running queries
mod runner;

/// Utilities for working with Prometheus.
mod metrics;

/// Prelude that exports the most important traits and types.
pub mod prelude {
    pub use super::execution::{ast as a, ExecutionContext, Query, Resolver};
    pub use super::introspection::IntrospectionResolver;
    pub use super::query::{execute_query, ext::BlockConstraint, QueryExecutionOptions};
    pub use super::store::StoreResolver;
    pub use super::subscription::SubscriptionExecutionOptions;
    pub use super::values::MaybeCoercible;

    pub use super::metrics::GraphQLMetrics;
    pub use super::runner::GraphQlRunner;
    pub use graph::prelude::s::ObjectType;
}

#[cfg(debug_assertions)]
pub mod test_support {
    pub use super::metrics::GraphQLMetrics;
    pub use super::runner::INITIAL_DEPLOYMENT_STATE_FOR_TESTS;
}
