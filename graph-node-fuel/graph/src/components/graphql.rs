use futures::prelude::*;

use crate::data::query::QueryResults;
use crate::data::query::{Query, QueryTarget};
use crate::data::subscription::{Subscription, SubscriptionError, SubscriptionResult};
use crate::prelude::DeploymentHash;

use async_trait::async_trait;
use std::sync::Arc;
use std::time::Duration;

/// Future for subscription results.
pub type SubscriptionResultFuture =
    Box<dyn Future<Item = SubscriptionResult, Error = SubscriptionError> + Send>;

pub enum GraphQlTarget {
    SubgraphName(String),
    Deployment(DeploymentHash),
}
/// A component that can run GraphqL queries against a [Store](../store/trait.Store.html).
#[async_trait]
pub trait GraphQlRunner: Send + Sync + 'static {
    /// Runs a GraphQL query and returns its result.
    async fn run_query(self: Arc<Self>, query: Query, target: QueryTarget) -> QueryResults;

    /// Runs a GraphqL query up to the given complexity. Overrides the global complexity limit.
    async fn run_query_with_complexity(
        self: Arc<Self>,
        query: Query,
        target: QueryTarget,
        max_complexity: Option<u64>,
        max_depth: Option<u8>,
        max_first: Option<u32>,
        max_skip: Option<u32>,
    ) -> QueryResults;

    /// Runs a GraphQL subscription and returns a stream of results.
    async fn run_subscription(
        self: Arc<Self>,
        subscription: Subscription,
        target: QueryTarget,
    ) -> Result<SubscriptionResult, SubscriptionError>;

    fn metrics(&self) -> Arc<dyn GraphQLMetrics>;
}

pub trait GraphQLMetrics: Send + Sync + 'static {
    fn observe_query_execution(&self, duration: Duration, results: &QueryResults);
    fn observe_query_parsing(&self, duration: Duration, results: &QueryResults);
    fn observe_query_validation(&self, duration: Duration, id: &DeploymentHash);
    fn observe_query_validation_error(&self, error_codes: Vec<&str>, id: &DeploymentHash);
    fn observe_query_blocks_behind(&self, blocks_behind: i32, id: &DeploymentHash);
}
