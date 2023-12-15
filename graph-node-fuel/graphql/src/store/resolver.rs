use std::collections::BTreeMap;
use std::result;
use std::sync::Arc;

use graph::components::graphql::GraphQLMetrics as _;
use graph::components::store::{QueryPermit, SubscriptionManager, UnitStream};
use graph::data::graphql::load_manager::LoadManager;
use graph::data::graphql::{object, ObjectOrInterface};
use graph::data::query::{CacheStatus, Trace};
use graph::data::value::{Object, Word};
use graph::prelude::*;
use graph::schema::{
    ast as sast, ApiSchema, INTROSPECTION_SCHEMA_FIELD_NAME, INTROSPECTION_TYPE_FIELD_NAME,
    META_FIELD_NAME, META_FIELD_TYPE,
};
use graph::schema::{ErrorPolicy, BLOCK_FIELD_TYPE};

use crate::execution::{ast as a, Query};
use crate::metrics::GraphQLMetrics;
use crate::prelude::{ExecutionContext, Resolver};
use crate::query::ext::BlockConstraint;
use crate::store::query::collect_entities_from_query_field;

/// A resolver that fetches entities from a `Store`.
#[derive(Clone)]
pub struct StoreResolver {
    #[allow(dead_code)]
    logger: Logger,
    pub(crate) store: Arc<dyn QueryStore>,
    subscription_manager: Arc<dyn SubscriptionManager>,
    pub(crate) block_ptr: Option<BlockPtrTs>,
    deployment: DeploymentHash,
    has_non_fatal_errors: bool,
    error_policy: ErrorPolicy,
    graphql_metrics: Arc<GraphQLMetrics>,
    load_manager: Arc<LoadManager>,
}

#[derive(Clone, Debug)]
pub(crate) struct BlockPtrTs {
    pub ptr: BlockPtr,
    pub timestamp: Option<u64>,
}

impl From<BlockPtr> for BlockPtrTs {
    fn from(ptr: BlockPtr) -> Self {
        Self {
            ptr,
            timestamp: None,
        }
    }
}

impl From<&BlockPtrTs> for BlockPtr {
    fn from(ptr: &BlockPtrTs) -> Self {
        ptr.ptr.cheap_clone()
    }
}

impl CheapClone for StoreResolver {}

impl StoreResolver {
    /// Create a resolver that looks up entities at whatever block is the
    /// latest when the query is run. That means that multiple calls to find
    /// entities into this resolver might return entities from different
    /// blocks
    pub fn for_subscription(
        logger: &Logger,
        deployment: DeploymentHash,
        store: Arc<dyn QueryStore>,
        subscription_manager: Arc<dyn SubscriptionManager>,
        graphql_metrics: Arc<GraphQLMetrics>,
        load_manager: Arc<LoadManager>,
    ) -> Self {
        StoreResolver {
            logger: logger.new(o!("component" => "StoreResolver")),
            store,
            subscription_manager,
            block_ptr: None,
            deployment,

            // Checking for non-fatal errors does not work with subscriptions.
            has_non_fatal_errors: false,
            error_policy: ErrorPolicy::Deny,
            graphql_metrics,
            load_manager,
        }
    }

    /// Create a resolver that looks up entities at the block specified
    /// by `bc`. Any calls to find objects will always return entities as
    /// of that block. Note that if `bc` is `BlockConstraint::Latest` we use
    /// whatever the latest block for the subgraph was when the resolver was
    /// created
    pub async fn at_block(
        logger: &Logger,
        store: Arc<dyn QueryStore>,
        state: &DeploymentState,
        subscription_manager: Arc<dyn SubscriptionManager>,
        bc: BlockConstraint,
        error_policy: ErrorPolicy,
        deployment: DeploymentHash,
        graphql_metrics: Arc<GraphQLMetrics>,
        load_manager: Arc<LoadManager>,
    ) -> Result<Self, QueryExecutionError> {
        let store_clone = store.cheap_clone();
        let block_ptr = Self::locate_block(store_clone.as_ref(), bc, state).await?;

        let blocks_behind = state.latest_block.number - block_ptr.ptr.number;
        graphql_metrics.observe_query_blocks_behind(blocks_behind, &deployment);

        let has_non_fatal_errors = store
            .has_deterministic_errors(block_ptr.ptr.block_number())
            .await?;

        let resolver = StoreResolver {
            logger: logger.new(o!("component" => "StoreResolver")),
            store,
            subscription_manager,
            block_ptr: Some(block_ptr),
            deployment,
            has_non_fatal_errors,
            error_policy,
            graphql_metrics,
            load_manager,
        };
        Ok(resolver)
    }

    pub fn block_number(&self) -> BlockNumber {
        self.block_ptr
            .as_ref()
            .map(|ptr| ptr.ptr.number as BlockNumber)
            .unwrap_or(BLOCK_NUMBER_MAX)
    }

    /// locate_block returns the block pointer and it's timestamp when available.
    async fn locate_block(
        store: &dyn QueryStore,
        bc: BlockConstraint,
        state: &DeploymentState,
    ) -> Result<BlockPtrTs, QueryExecutionError> {
        fn block_queryable(
            state: &DeploymentState,
            block: BlockNumber,
        ) -> Result<(), QueryExecutionError> {
            state
                .block_queryable(block)
                .map_err(|msg| QueryExecutionError::ValueParseError("block.number".to_owned(), msg))
        }

        async fn get_block_ts(
            store: &dyn QueryStore,
            ptr: &BlockPtr,
        ) -> Result<Option<u64>, QueryExecutionError> {
            match store
                .block_number_with_timestamp(&ptr.hash)
                .await
                .map_err(Into::<QueryExecutionError>::into)?
            {
                Some((_, Some(ts))) => Ok(Some(ts)),
                _ => Ok(None),
            }
        }

        match bc {
            BlockConstraint::Hash(hash) => {
                let ptr = store
                    .block_number_with_timestamp(&hash)
                    .await
                    .map_err(Into::into)
                    .and_then(|result| {
                        result
                            .ok_or_else(|| {
                                QueryExecutionError::ValueParseError(
                                    "block.hash".to_owned(),
                                    "no block with that hash found".to_owned(),
                                )
                            })
                            .map(|(number, ts)| BlockPtrTs {
                                ptr: BlockPtr::new(hash, number),
                                timestamp: ts,
                            })
                    })?;

                block_queryable(state, ptr.ptr.number)?;
                Ok(ptr)
            }
            BlockConstraint::Number(number) => {
                block_queryable(state, number)?;
                // We don't have a way here to look the block hash up from
                // the database, and even if we did, there is no guarantee
                // that we have the block in our cache. We therefore
                // always return an all zeroes hash when users specify
                // a block number
                // See 7a7b9708-adb7-4fc2-acec-88680cb07ec1
                Ok(BlockPtr::from((web3::types::H256::zero(), number as u64)).into())
            }
            BlockConstraint::Min(min) => {
                let ptr = state.latest_block.cheap_clone();
                if ptr.number < min {
                    return Err(QueryExecutionError::ValueParseError(
                        "block.number_gte".to_owned(),
                        format!(
                            "subgraph {} has only indexed up to block number {} \
                                and data for block number {} is therefore not yet available",
                            state.id, ptr.number, min
                        ),
                    ));
                }
                let timestamp = get_block_ts(store, &state.latest_block).await?;

                Ok(BlockPtrTs { ptr, timestamp })
            }
            BlockConstraint::Latest => {
                let timestamp = get_block_ts(store, &state.latest_block).await?;

                Ok(BlockPtrTs {
                    ptr: state.latest_block.cheap_clone(),
                    timestamp,
                })
            }
        }
    }

    fn handle_meta(
        &self,
        prefetched_object: Option<r::Value>,
        object_type: &ObjectOrInterface<'_>,
    ) -> Result<(Option<r::Value>, Option<r::Value>), QueryExecutionError> {
        // Pretend that the whole `_meta` field was loaded by prefetch. Eager
        // loading this is ok until we add more information to this field
        // that would force us to query the database; when that happens, we
        // need to switch to loading on demand
        if object_type.is_meta() {
            let hash = self
                .block_ptr
                .as_ref()
                .and_then(|ptr| {
                    // locate_block indicates that we do not have a block hash
                    // by setting the hash to `zero`
                    // See 7a7b9708-adb7-4fc2-acec-88680cb07ec1
                    let hash_h256 = ptr.ptr.hash_as_h256();
                    if hash_h256 == web3::types::H256::zero() {
                        None
                    } else {
                        Some(r::Value::String(format!("0x{:x}", hash_h256)))
                    }
                })
                .unwrap_or(r::Value::Null);
            let number = self
                .block_ptr
                .as_ref()
                .map(|ptr| r::Value::Int(ptr.ptr.number.into()))
                .unwrap_or(r::Value::Null);

            let timestamp = self.block_ptr.as_ref().map(|ptr| {
                ptr.timestamp
                    .map(|ts| r::Value::Int(ts as i64))
                    .unwrap_or(r::Value::Null)
            });

            let mut map = BTreeMap::new();
            let block = object! {
                hash: hash,
                number: number,
                timestamp: timestamp,
                __typename: BLOCK_FIELD_TYPE
            };
            map.insert("prefetch:block".into(), r::Value::List(vec![block]));
            map.insert(
                "deployment".into(),
                r::Value::String(self.deployment.to_string()),
            );
            map.insert(
                "hasIndexingErrors".into(),
                r::Value::Boolean(self.has_non_fatal_errors),
            );
            map.insert(
                "__typename".into(),
                r::Value::String(META_FIELD_TYPE.to_string()),
            );
            return Ok((None, Some(r::Value::object(map))));
        }
        Ok((prefetched_object, None))
    }
}

#[async_trait]
impl Resolver for StoreResolver {
    const CACHEABLE: bool = true;

    async fn query_permit(&self) -> Result<QueryPermit, QueryExecutionError> {
        self.store.query_permit().await.map_err(Into::into)
    }

    fn prefetch(
        &self,
        ctx: &ExecutionContext<Self>,
        selection_set: &a::SelectionSet,
    ) -> Result<(Option<r::Value>, Trace), Vec<QueryExecutionError>> {
        super::prefetch::run(self, ctx, selection_set, &self.graphql_metrics)
            .map(|(value, trace)| (Some(value), trace))
    }

    async fn resolve_objects(
        &self,
        prefetched_objects: Option<r::Value>,
        field: &a::Field,
        _field_definition: &s::Field,
        object_type: ObjectOrInterface<'_>,
    ) -> Result<r::Value, QueryExecutionError> {
        if let Some(child) = prefetched_objects {
            Ok(child)
        } else {
            Err(QueryExecutionError::ResolveEntitiesError(format!(
                "internal error resolving {}.{}: \
                 expected prefetched result, but found nothing",
                object_type.name(),
                &field.name,
            )))
        }
    }

    async fn resolve_object(
        &self,
        prefetched_object: Option<r::Value>,
        field: &a::Field,
        field_definition: &s::Field,
        object_type: ObjectOrInterface<'_>,
    ) -> Result<r::Value, QueryExecutionError> {
        let (prefetched_object, meta) = self.handle_meta(prefetched_object, &object_type)?;
        if let Some(meta) = meta {
            return Ok(meta);
        }
        if let Some(r::Value::List(children)) = prefetched_object {
            if children.len() > 1 {
                let derived_from_field =
                    sast::get_derived_from_field(object_type, field_definition)
                        .expect("only derived fields can lead to multiple children here");

                return Err(QueryExecutionError::AmbiguousDerivedFromResult(
                    field.position,
                    field.name.clone(),
                    object_type.name().to_owned(),
                    derived_from_field.name.clone(),
                ));
            } else {
                Ok(children.into_iter().next().unwrap_or(r::Value::Null))
            }
        } else {
            return Err(QueryExecutionError::ResolveEntitiesError(format!(
                "internal error resolving {}.{}: \
                 expected prefetched result, but found nothing",
                object_type.name(),
                &field.name,
            )));
        }
    }

    fn resolve_field_stream(
        &self,
        schema: &ApiSchema,
        object_type: &s::ObjectType,
        field: &a::Field,
    ) -> result::Result<UnitStream, QueryExecutionError> {
        // Collect all entities involved in the query field
        let object_type = schema.object_type(object_type).into();
        let input_schema = self.store.input_schema()?;
        let entities =
            collect_entities_from_query_field(&input_schema, schema, object_type, field)?;

        // Subscribe to the store and return the entity change stream
        Ok(self.subscription_manager.subscribe_no_payload(entities))
    }

    fn post_process(&self, result: &mut QueryResult) -> Result<(), anyhow::Error> {
        // Post-processing is only necessary for queries with indexing errors, and no query errors.
        if !self.has_non_fatal_errors || result.has_errors() {
            return Ok(());
        }

        // Add the "indexing_error" to the response.
        assert!(result.errors_mut().is_empty());
        *result.errors_mut() = vec![QueryError::IndexingError];

        match self.error_policy {
            // If indexing errors are denied, we omit results, except for the `_meta` response.
            // Note that the meta field could have been queried under a different response key,
            // or a different field queried under the response key `_meta`.
            ErrorPolicy::Deny => {
                let mut data = result.take_data();

                // Only keep the _meta, __schema and __type fields from the data
                let meta_fields = data.as_mut().and_then(|d| {
                    let meta_field = d.remove(META_FIELD_NAME);
                    let schema_field = d.remove(INTROSPECTION_SCHEMA_FIELD_NAME);
                    let type_field = d.remove(INTROSPECTION_TYPE_FIELD_NAME);

                    // combine the fields into a vector
                    let mut meta_fields = Vec::new();

                    if let Some(meta_field) = meta_field {
                        meta_fields.push((Word::from(META_FIELD_NAME), meta_field));
                    }
                    if let Some(schema_field) = schema_field {
                        meta_fields
                            .push((Word::from(INTROSPECTION_SCHEMA_FIELD_NAME), schema_field));
                    }
                    if let Some(type_field) = type_field {
                        meta_fields.push((Word::from(INTROSPECTION_TYPE_FIELD_NAME), type_field));
                    }

                    // return the object if it is not empty
                    if meta_fields.is_empty() {
                        None
                    } else {
                        Some(Object::from_iter(meta_fields))
                    }
                });

                result.set_data(meta_fields);
            }
            ErrorPolicy::Allow => (),
        }
        Ok(())
    }

    fn record_work(&self, query: &Query, elapsed: Duration, cache_status: CacheStatus) {
        self.load_manager.record_work(
            self.store.shard(),
            self.store.deployment_id(),
            query.shape_hash,
            elapsed,
            cache_status,
        );
    }
}
