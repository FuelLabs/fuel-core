use std::result::Result;
use std::time::{Duration, Instant};

use graph::components::store::UnitStream;
use graph::data::graphql::load_manager::LoadManager;
use graph::schema::ApiSchema;
use graph::{components::store::SubscriptionManager, prelude::*, schema::ErrorPolicy};

use crate::metrics::GraphQLMetrics;
use crate::{
    execution::ast as a,
    execution::*,
    prelude::{BlockConstraint, StoreResolver},
};

/// Options available for subscription execution.
pub struct SubscriptionExecutionOptions {
    /// The logger to use during subscription execution.
    pub logger: Logger,

    /// The store to use.
    pub store: Arc<dyn QueryStore>,

    pub subscription_manager: Arc<dyn SubscriptionManager>,

    /// Individual timeout for each subscription query.
    pub timeout: Option<Duration>,

    /// Maximum complexity for a subscription query.
    pub max_complexity: Option<u64>,

    /// Maximum depth for a subscription query.
    pub max_depth: u8,

    /// Maximum value for the `first` argument.
    pub max_first: u32,

    /// Maximum value for the `skip` argument.
    pub max_skip: u32,

    pub graphql_metrics: Arc<GraphQLMetrics>,

    pub load_manager: Arc<LoadManager>,
}

pub fn execute_subscription(
    subscription: Subscription,
    schema: Arc<ApiSchema>,
    options: SubscriptionExecutionOptions,
) -> Result<SubscriptionResult, SubscriptionError> {
    let query = crate::execution::Query::new(
        &options.logger,
        schema,
        None,
        subscription.query,
        options.max_complexity,
        options.max_depth,
        options.graphql_metrics.cheap_clone(),
    )?;
    execute_prepared_subscription(query, options)
}

pub(crate) fn execute_prepared_subscription(
    query: Arc<crate::execution::Query>,
    options: SubscriptionExecutionOptions,
) -> Result<SubscriptionResult, SubscriptionError> {
    if !query.is_subscription() {
        return Err(SubscriptionError::from(QueryExecutionError::NotSupported(
            "Only subscriptions are supported".to_string(),
        )));
    }

    info!(
        options.logger,
        "Execute subscription";
        "query" => &query.query_text,
    );

    let source_stream = create_source_event_stream(query.clone(), &options)?;
    let response_stream = map_source_to_response_stream(query, options, source_stream);
    Ok(response_stream)
}

fn create_source_event_stream(
    query: Arc<crate::execution::Query>,
    options: &SubscriptionExecutionOptions,
) -> Result<UnitStream, SubscriptionError> {
    let resolver = StoreResolver::for_subscription(
        &options.logger,
        query.schema.id().clone(),
        options.store.clone(),
        options.subscription_manager.cheap_clone(),
        options.graphql_metrics.cheap_clone(),
        options.load_manager.cheap_clone(),
    );
    let ctx = ExecutionContext {
        logger: options.logger.cheap_clone(),
        resolver,
        query,
        deadline: None,
        max_first: options.max_first,
        max_skip: options.max_skip,
        cache_status: Default::default(),
        trace: ENV_VARS.log_sql_timing(),
    };

    let subscription_type = ctx
        .query
        .schema
        .subscription_type
        .as_ref()
        .ok_or(QueryExecutionError::NoRootSubscriptionObjectType)?;

    let field = if ctx.query.selection_set.is_empty() {
        return Err(SubscriptionError::from(QueryExecutionError::EmptyQuery));
    } else {
        match ctx.query.selection_set.single_field() {
            Some(field) => field,
            None => {
                return Err(SubscriptionError::from(
                    QueryExecutionError::MultipleSubscriptionFields,
                ));
            }
        }
    };

    resolve_field_stream(&ctx, subscription_type, field)
}

fn resolve_field_stream(
    ctx: &ExecutionContext<impl Resolver>,
    object_type: &s::ObjectType,
    field: &a::Field,
) -> Result<UnitStream, SubscriptionError> {
    ctx.resolver
        .resolve_field_stream(&ctx.query.schema, object_type, field)
        .map_err(SubscriptionError::from)
}

fn map_source_to_response_stream(
    query: Arc<crate::execution::Query>,
    options: SubscriptionExecutionOptions,
    source_stream: UnitStream,
) -> QueryResultStream {
    // Create a stream with a single empty event. By chaining this in front
    // of the real events, we trick the subscription into executing its query
    // at least once. This satisfies the GraphQL over Websocket protocol
    // requirement of "respond[ing] with at least one GQL_DATA message", see
    // https://github.com/apollographql/subscriptions-transport-ws/blob/master/PROTOCOL.md#gql_data
    let trigger_stream = futures03::stream::once(async {});

    let SubscriptionExecutionOptions {
        logger,
        store,
        subscription_manager,
        timeout,
        max_complexity: _,
        max_depth: _,
        max_first,
        max_skip,
        graphql_metrics,
        load_manager,
    } = options;

    trigger_stream
        .chain(source_stream)
        .then(move |()| {
            execute_subscription_event(
                logger.clone(),
                store.clone(),
                subscription_manager.cheap_clone(),
                query.clone(),
                timeout,
                max_first,
                max_skip,
                graphql_metrics.cheap_clone(),
                load_manager.cheap_clone(),
            )
            .boxed()
        })
        .boxed()
}

async fn execute_subscription_event(
    logger: Logger,
    store: Arc<dyn QueryStore>,
    subscription_manager: Arc<dyn SubscriptionManager>,
    query: Arc<crate::execution::Query>,
    timeout: Option<Duration>,
    max_first: u32,
    max_skip: u32,
    metrics: Arc<GraphQLMetrics>,
    load_manager: Arc<LoadManager>,
) -> Arc<QueryResult> {
    async fn make_resolver(
        store: Arc<dyn QueryStore>,
        logger: &Logger,
        subscription_manager: Arc<dyn SubscriptionManager>,
        query: &Arc<crate::execution::Query>,
        metrics: Arc<GraphQLMetrics>,
        load_manager: Arc<LoadManager>,
    ) -> Result<StoreResolver, QueryExecutionError> {
        let state = store.deployment_state().await?;
        StoreResolver::at_block(
            logger,
            store,
            &state,
            subscription_manager,
            BlockConstraint::Latest,
            ErrorPolicy::Deny,
            query.schema.id().clone(),
            metrics,
            load_manager,
        )
        .await
    }

    let resolver = match make_resolver(
        store,
        &logger,
        subscription_manager,
        &query,
        metrics,
        load_manager,
    )
    .await
    {
        Ok(resolver) => resolver,
        Err(e) => return Arc::new(e.into()),
    };

    let block_ptr = resolver.block_ptr.as_ref().map(Into::into);

    // Create a fresh execution context with deadline.
    let ctx = Arc::new(ExecutionContext {
        logger,
        resolver,
        query,
        deadline: timeout.map(|t| Instant::now() + t),
        max_first,
        max_skip,
        cache_status: Default::default(),
        trace: ENV_VARS.log_sql_timing(),
    });

    let subscription_type = match ctx.query.schema.subscription_type.as_ref() {
        Some(t) => t.cheap_clone(),
        None => return Arc::new(QueryExecutionError::NoRootSubscriptionObjectType.into()),
    };

    execute_root_selection_set(
        ctx.cheap_clone(),
        ctx.query.selection_set.cheap_clone(),
        subscription_type.into(),
        block_ptr,
    )
    .await
}
