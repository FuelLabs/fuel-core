use std::sync::Arc;
use std::time::Instant;

use crate::metrics::GraphQLMetrics;
use crate::prelude::{QueryExecutionOptions, StoreResolver, SubscriptionExecutionOptions};
use crate::query::execute_query;
use crate::subscription::execute_prepared_subscription;
use graph::prelude::MetricsRegistry;
use graph::{
    components::store::SubscriptionManager,
    prelude::{
        async_trait, o, CheapClone, DeploymentState, GraphQLMetrics as GraphQLMetricsTrait,
        GraphQlRunner as GraphQlRunnerTrait, Logger, Query, QueryExecutionError, Subscription,
        SubscriptionError, SubscriptionResult, ENV_VARS,
    },
};
use graph::{data::graphql::load_manager::LoadManager, prelude::QueryStoreManager};
use graph::{
    data::query::{QueryResults, QueryTarget},
    prelude::QueryStore,
};

/// GraphQL runner implementation for The Graph.
pub struct GraphQlRunner<S, SM> {
    logger: Logger,
    store: Arc<S>,
    subscription_manager: Arc<SM>,
    load_manager: Arc<LoadManager>,
    graphql_metrics: Arc<GraphQLMetrics>,
}

#[cfg(debug_assertions)]
lazy_static::lazy_static! {
    // Test only, see c435c25decbc4ad7bbbadf8e0ced0ff2
    pub static ref INITIAL_DEPLOYMENT_STATE_FOR_TESTS: std::sync::Mutex<Option<DeploymentState>> = std::sync::Mutex::new(None);
}

impl<S, SM> GraphQlRunner<S, SM>
where
    S: QueryStoreManager,
    SM: SubscriptionManager,
{
    /// Creates a new query runner.
    pub fn new(
        logger: &Logger,
        store: Arc<S>,
        subscription_manager: Arc<SM>,
        load_manager: Arc<LoadManager>,
        registry: Arc<MetricsRegistry>,
    ) -> Self {
        let logger = logger.new(o!("component" => "GraphQlRunner"));
        let graphql_metrics = Arc::new(GraphQLMetrics::new(registry));
        GraphQlRunner {
            logger,
            store,
            subscription_manager,
            load_manager,
            graphql_metrics,
        }
    }

    /// Check if the subgraph state differs from `state` now in a way that
    /// would affect a query that looked at data as fresh as `latest_block`.
    /// If the subgraph did change, return the `Err` that should be sent back
    /// to clients to indicate that condition
    async fn deployment_changed(
        &self,
        store: &dyn QueryStore,
        state: DeploymentState,
        latest_block: u64,
    ) -> Result<(), QueryExecutionError> {
        if ENV_VARS.graphql.allow_deployment_change {
            return Ok(());
        }
        let new_state = store.deployment_state().await?;
        assert!(new_state.reorg_count >= state.reorg_count);
        if new_state.reorg_count > state.reorg_count {
            // One or more reorgs happened; each reorg can't have gone back
            // farther than `max_reorg_depth`, so that querying at blocks
            // far enough away from the previous latest block is fine. Taking
            // this into consideration is important, since most of the time
            // there is only one reorg of one block, and we therefore avoid
            // flagging a lot of queries a bit behind the head
            let n_blocks = new_state.max_reorg_depth * (new_state.reorg_count - state.reorg_count);
            if latest_block + n_blocks as u64 > state.latest_block.number as u64 {
                return Err(QueryExecutionError::DeploymentReverted);
            }
        }
        Ok(())
    }

    async fn execute(
        &self,
        query: Query,
        target: QueryTarget,
        max_complexity: Option<u64>,
        max_depth: Option<u8>,
        max_first: Option<u32>,
        max_skip: Option<u32>,
        metrics: Arc<GraphQLMetrics>,
    ) -> Result<QueryResults, QueryResults> {
        // We need to use the same `QueryStore` for the entire query to ensure
        // we have a consistent view if the world, even when replicas, which
        // are eventually consistent, are in use. If we run different parts
        // of the query against different replicas, it would be possible for
        // them to be at wildly different states, and we might unwittingly
        // mix data from different block heights even if no reverts happen
        // while the query is running. `self.store` can not be used after this
        // point, and everything needs to go through the `store` we are
        // setting up here

        let store = self.store.query_store(target.clone(), false).await?;
        let state = store.deployment_state().await?;
        let network = Some(store.network_name().to_string());
        let schema = store.api_schema()?;

        // Test only, see c435c25decbc4ad7bbbadf8e0ced0ff2
        #[cfg(debug_assertions)]
        let state = INITIAL_DEPLOYMENT_STATE_FOR_TESTS
            .lock()
            .unwrap()
            .clone()
            .unwrap_or(state);

        let max_depth = max_depth.unwrap_or(ENV_VARS.graphql.max_depth);
        let trace = query.trace;
        let query = crate::execution::Query::new(
            &self.logger,
            schema,
            network,
            query,
            max_complexity,
            max_depth,
            metrics.cheap_clone(),
        )?;
        self.load_manager
            .decide(
                &store.wait_stats().map_err(QueryExecutionError::from)?,
                store.shard(),
                store.deployment_id(),
                query.shape_hash,
                query.query_text.as_ref(),
            )
            .to_result()?;
        let by_block_constraint = query.block_constraint()?;
        let mut max_block = 0;
        let mut result: QueryResults = QueryResults::empty();

        // Note: This will always iterate at least once.
        for (bc, (selection_set, error_policy)) in by_block_constraint {
            let query_start = Instant::now();
            let resolver = StoreResolver::at_block(
                &self.logger,
                store.cheap_clone(),
                &state,
                self.subscription_manager.cheap_clone(),
                bc,
                error_policy,
                query.schema.id().clone(),
                metrics.cheap_clone(),
                self.load_manager.cheap_clone(),
            )
            .await?;
            max_block = max_block.max(resolver.block_number());
            let query_res = execute_query(
                query.clone(),
                Some(selection_set),
                resolver.block_ptr.as_ref().map(Into::into).clone(),
                QueryExecutionOptions {
                    resolver,
                    deadline: ENV_VARS.graphql.query_timeout.map(|t| Instant::now() + t),
                    max_first: max_first.unwrap_or(ENV_VARS.graphql.max_first),
                    max_skip: max_skip.unwrap_or(ENV_VARS.graphql.max_skip),
                    trace,
                },
            )
            .await;
            query_res.trace.finish(query_start.elapsed());
            result.append(query_res);
        }

        query.log_execution(max_block);
        self.deployment_changed(store.as_ref(), state, max_block as u64)
            .await
            .map_err(QueryResults::from)
            .map(|()| result)
    }
}

#[async_trait]
impl<S, SM> GraphQlRunnerTrait for GraphQlRunner<S, SM>
where
    S: QueryStoreManager,
    SM: SubscriptionManager,
{
    async fn run_query(self: Arc<Self>, query: Query, target: QueryTarget) -> QueryResults {
        self.run_query_with_complexity(
            query,
            target,
            ENV_VARS.graphql.max_complexity,
            Some(ENV_VARS.graphql.max_depth),
            Some(ENV_VARS.graphql.max_first),
            Some(ENV_VARS.graphql.max_skip),
        )
        .await
    }

    async fn run_query_with_complexity(
        self: Arc<Self>,
        query: Query,
        target: QueryTarget,
        max_complexity: Option<u64>,
        max_depth: Option<u8>,
        max_first: Option<u32>,
        max_skip: Option<u32>,
    ) -> QueryResults {
        self.execute(
            query,
            target,
            max_complexity,
            max_depth,
            max_first,
            max_skip,
            self.graphql_metrics.clone(),
        )
        .await
        .unwrap_or_else(|e| e)
    }

    async fn run_subscription(
        self: Arc<Self>,
        subscription: Subscription,
        target: QueryTarget,
    ) -> Result<SubscriptionResult, SubscriptionError> {
        let store = self.store.query_store(target.clone(), true).await?;
        let schema = store.api_schema()?;
        let network = store.network_name().to_string();

        let query = crate::execution::Query::new(
            &self.logger,
            schema,
            Some(network),
            subscription.query,
            ENV_VARS.graphql.max_complexity,
            ENV_VARS.graphql.max_depth,
            self.graphql_metrics.cheap_clone(),
        )?;

        if let Err(err) = self
            .load_manager
            .decide(
                &store.wait_stats().map_err(QueryExecutionError::from)?,
                store.shard(),
                store.deployment_id(),
                query.shape_hash,
                query.query_text.as_ref(),
            )
            .to_result()
        {
            return Err(SubscriptionError::GraphQLError(vec![err]));
        }

        execute_prepared_subscription(
            query,
            SubscriptionExecutionOptions {
                logger: self.logger.clone(),
                store,
                subscription_manager: self.subscription_manager.cheap_clone(),
                timeout: ENV_VARS.graphql.query_timeout,
                max_complexity: ENV_VARS.graphql.max_complexity,
                max_depth: ENV_VARS.graphql.max_depth,
                max_first: ENV_VARS.graphql.max_first,
                max_skip: ENV_VARS.graphql.max_skip,
                graphql_metrics: self.graphql_metrics.clone(),
                load_manager: self.load_manager.cheap_clone(),
            },
        )
    }

    fn metrics(&self) -> Arc<dyn GraphQLMetricsTrait> {
        self.graphql_metrics.clone()
    }
}
