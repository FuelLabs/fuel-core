//! Run a GraphQL query and fetch all the entitied needed to build the
//! final result

use graph::constraint_violation;
use graph::data::query::Trace;
use graph::data::store::Id;
use graph::data::store::IdList;
use graph::data::store::IdType;
use graph::data::store::QueryObject;
use graph::data::value::{Object, Word};
use graph::prelude::{r, CacheWeight, CheapClone};
use graph::slog::warn;
use graph::util::cache_weight;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;
use std::time::Instant;

use graph::data::graphql::*;
use graph::schema::{ast as sast, EntityType, InputSchema};
use graph::{
    data::graphql::ext::DirectiveFinder,
    prelude::{
        s, AttributeNames, ChildMultiplicity, EntityCollection, EntityFilter, EntityLink,
        EntityOrder, EntityWindow, ParentLink, QueryExecutionError, Value as StoreValue,
        WindowAttribute, ENV_VARS,
    },
};

use crate::execution::{ast as a, ExecutionContext, Resolver};
use crate::metrics::GraphQLMetrics;
use crate::store::query::build_query;
use crate::store::StoreResolver;

pub const ARG_ID: &str = "id";

/// Intermediate data structure to hold the results of prefetching entities
/// and their nested associations. For each association of `entity`, `children`
/// has an entry mapping the response key to the list of nodes.
#[derive(Debug, Clone)]
struct Node {
    /// Estimate the size of the children using their `CacheWeight`. This
    /// field will have the cache weight of the `entity` plus the weight of
    /// the keys and values of the `children` map, but not of the map itself
    children_weight: usize,

    parent: Option<Id>,

    entity: Object,
    /// We are using an `Rc` here for two reasons: it allows us to defer
    /// copying objects until the end, when converting to `q::Value` forces
    /// us to copy any child that is referenced by multiple parents. It also
    /// makes it possible to avoid unnecessary copying of a child that is
    /// referenced by only one parent - without the `Rc` we would have to
    /// copy since we do not know that only one parent uses it.
    ///
    /// Multiple parents can reference a single child in the following
    /// situation: assume a GraphQL query `balances { token { issuer {id}}}`
    /// where `balances` stores the `id` of the `token`, and `token` stores
    /// the `id` of its `issuer`. Execution of the query when all `balances`
    /// reference the same `token` will happen through several invocations
    /// of `fetch`. For the purposes of this comment, we can think of
    /// `fetch` as taking a list of `(parent_id, child_id)` pairs and
    /// returning entities that are identified by this pair, i.e., there
    /// will be one entity for each unique `(parent_id, child_id)`
    /// combination, rather than one for each unique `child_id`. In reality,
    /// of course, we will usually not know the `child_id` yet until we
    /// actually run the query.
    ///
    /// Query execution works as follows:
    /// 1. Fetch all `balances`, returning `#b` `Balance` entities. The
    ///    `Balance.token` field will be the same for all these entities.
    /// 2. Fetch `#b` `Token` entities, identified through `(Balance.id,
    ///    Balance.token)` resulting in one `Token` entity
    /// 3. Fetch 1 `Issuer` entity, identified through `(Token.id,
    ///    Token.issuer)`
    /// 4. Glue all these results together into a DAG through invocations of
    ///    `Join::perform`
    ///
    /// We now have `#b` `Node` instances representing the same `Token`, but
    /// each the child of a different `Node` for the `#b` balances. Each of
    /// those `#b` `Token` nodes points to the same `Issuer` node. It's
    /// important to note that the issuer node could itself be the root of a
    /// large tree and could therefore take up a lot of memory. When we
    /// convert this DAG into `q::Value`, we need to make `#b` copies of the
    /// `Issuer` node. Using an `Rc` in `Node` allows us to defer these
    /// copies to the point where we need to convert to `q::Value`, and it
    /// would be desirable to base the data structure that GraphQL execution
    /// uses on a DAG rather than a tree, but that's a good amount of work
    children: BTreeMap<Word, Vec<Rc<Node>>>,
}

impl From<QueryObject> for Node {
    fn from(object: QueryObject) -> Self {
        Node {
            children_weight: object.weight(),
            parent: object.parent,
            entity: object.entity,
            children: BTreeMap::default(),
        }
    }
}

impl CacheWeight for Node {
    fn indirect_weight(&self) -> usize {
        self.children_weight + cache_weight::btree::node_size(&self.children)
    }
}

/// Convert a list of nodes into a `q::Value::List` where each node has also
/// been converted to a `q::Value`
fn node_list_as_value(nodes: Vec<Rc<Node>>) -> r::Value {
    r::Value::List(
        nodes
            .into_iter()
            .map(|node| Rc::try_unwrap(node).unwrap_or_else(|rc| rc.as_ref().clone()))
            .map(Into::into)
            .collect(),
    )
}

/// We pass the root node of the result around as a vec of nodes, not as
/// a single node so that we can use the same functions on interior node
/// lists which are the result of querying the database. The root list
/// consists of exactly one entry, and that entry has an empty
/// (not even a `__typename`) entity.
///
/// That distinguishes it from both the result of a query that matches
/// nothing (an empty `Vec`), and a result that finds just one entity
/// (the entity is not completely empty)
fn is_root_node<'a>(mut nodes: impl Iterator<Item = &'a Node>) -> bool {
    if let Some(node) = nodes.next() {
        node.entity.is_empty()
    } else {
        false
    }
}

fn make_root_node() -> Vec<Node> {
    let entity = Object::empty();
    vec![Node {
        children_weight: entity.weight(),
        parent: None,
        entity,
        children: BTreeMap::default(),
    }]
}

/// Recursively convert a `Node` into the corresponding `q::Value`, which is
/// always a `q::Value::Object`. The entity's associations are mapped to
/// entries `r:{response_key}` as that name is guaranteed to not conflict
/// with any field of the entity.
impl From<Node> for r::Value {
    fn from(node: Node) -> Self {
        let mut map = node.entity;
        let entries = node.children.into_iter().map(|(key, nodes)| {
            (
                format!("prefetch:{}", key).into(),
                node_list_as_value(nodes),
            )
        });
        map.extend(entries);
        r::Value::Object(map)
    }
}

trait ValueExt {
    fn as_str(&self) -> Option<&str>;
    fn as_id(&self, id_type: IdType) -> Option<Id>;
}

impl ValueExt for r::Value {
    fn as_str(&self) -> Option<&str> {
        match self {
            r::Value::String(s) => Some(s),
            _ => None,
        }
    }

    fn as_id(&self, id_type: IdType) -> Option<Id> {
        match self {
            r::Value::String(s) => id_type.parse(Word::from(s.as_str())).ok(),
            _ => None,
        }
    }
}

impl Node {
    fn id(&self, schema: &InputSchema) -> Result<Id, QueryExecutionError> {
        let entity_type = schema.entity_type(self.typename())?;
        match self.get("id") {
            None => Err(QueryExecutionError::IdMissing),
            Some(r::Value::String(s)) => {
                let id = entity_type.parse_id(s.as_str())?;
                Ok(id)
            }
            _ => Err(QueryExecutionError::IdNotString),
        }
    }

    fn get(&self, key: &str) -> Option<&r::Value> {
        self.entity.get(key)
    }

    fn typename(&self) -> &str {
        self.get("__typename")
            .expect("all entities have a __typename")
            .as_str()
            .expect("__typename must be a string")
    }

    fn set_children(&mut self, response_key: String, nodes: Vec<Rc<Node>>) {
        fn nodes_weight(nodes: &Vec<Rc<Node>>) -> usize {
            let vec_weight = nodes.capacity() * std::mem::size_of::<Rc<Node>>();
            let children_weight = nodes.iter().map(|node| node.weight()).sum::<usize>();
            vec_weight + children_weight
        }

        let key_weight = response_key.weight();

        self.children_weight += nodes_weight(&nodes) + key_weight;
        let old = self.children.insert(response_key.into(), nodes);
        if let Some(old) = old {
            self.children_weight -= nodes_weight(&old) + key_weight;
        }
    }
}

/// Describe a field that we join on. The distinction between scalar and
/// list is important for generating the right filter, and handling results
/// correctly
#[derive(Debug)]
enum JoinField<'a> {
    List(&'a str),
    Scalar(&'a str),
}

impl<'a> JoinField<'a> {
    fn new(field: &'a s::Field) -> Self {
        let name = field.name.as_str();
        if sast::is_list_or_non_null_list_field(field) {
            JoinField::List(name)
        } else {
            JoinField::Scalar(name)
        }
    }

    fn window_attribute(&self) -> WindowAttribute {
        match self {
            JoinField::Scalar(name) => WindowAttribute::Scalar(name.to_string()),
            JoinField::List(name) => WindowAttribute::List(name.to_string()),
        }
    }
}

#[derive(Debug)]
enum JoinRelation<'a> {
    // Name of field in which child stores parent ids
    Direct(JoinField<'a>),
    // Name of the field in the parent type containing child ids
    Derived(JoinField<'a>),
}

#[derive(Debug)]
struct JoinCond<'a> {
    /// The (concrete) object type of the parent, interfaces will have
    /// one `JoinCond` for each implementing type
    parent_type: EntityType,
    /// The (concrete) object type of the child, interfaces will have
    /// one `JoinCond` for each implementing type
    child_type: EntityType,
    relation: JoinRelation<'a>,
}

impl<'a> JoinCond<'a> {
    fn new(
        schema: &InputSchema,
        parent_type: &'a s::ObjectType,
        child_type: &'a s::ObjectType,
        field_name: &str,
    ) -> Self {
        let field = parent_type
            .field(field_name)
            .expect("field_name is a valid field of parent_type");
        let relation =
            if let Some(derived_from_field) = sast::get_derived_from_field(child_type, field) {
                JoinRelation::Direct(JoinField::new(derived_from_field))
            } else {
                JoinRelation::Derived(JoinField::new(field))
            };
        JoinCond {
            parent_type: schema.entity_type(parent_type).unwrap(),
            child_type: schema.entity_type(child_type).unwrap(),
            relation,
        }
    }

    fn entity_link(
        &self,
        parents_by_id: Vec<(Id, &Node)>,
        multiplicity: ChildMultiplicity,
    ) -> Result<(IdList, EntityLink), QueryExecutionError> {
        match &self.relation {
            JoinRelation::Direct(field) => {
                // we only need the parent ids
                let ids = IdList::try_from_iter(
                    &self.parent_type,
                    parents_by_id.into_iter().map(|(id, _)| id),
                )?;
                Ok((
                    ids,
                    EntityLink::Direct(field.window_attribute(), multiplicity),
                ))
            }
            JoinRelation::Derived(field) => {
                let (ids, parent_link) = match field {
                    JoinField::Scalar(child_field) => {
                        // child_field contains a String id of the child; extract
                        // those and the parent ids
                        let id_type = self.child_type.id_type().unwrap();
                        let (ids, child_ids): (Vec<_>, Vec<_>) = parents_by_id
                            .into_iter()
                            .filter_map(|(id, node)| {
                                node.get(child_field)
                                    .and_then(|value| value.as_id(id_type))
                                    .map(|child_id| (id, child_id.to_owned()))
                            })
                            .unzip();
                        let ids = IdList::try_from_iter(&self.parent_type, ids.into_iter())?;
                        let child_ids =
                            IdList::try_from_iter(&self.child_type, child_ids.into_iter())?;
                        (ids, ParentLink::Scalar(child_ids))
                    }
                    JoinField::List(child_field) => {
                        // child_field stores a list of child ids; extract them,
                        // turn them into a list of strings and combine with the
                        // parent ids
                        let id_type = self.child_type.id_type().unwrap();
                        let (ids, child_ids): (Vec<_>, Vec<_>) = parents_by_id
                            .into_iter()
                            .filter_map(|(id, node)| {
                                node.get(child_field)
                                    .and_then(|value| match value {
                                        r::Value::List(values) => {
                                            let values: Vec<_> = values
                                                .iter()
                                                .filter_map(|value| value.as_id(id_type))
                                                .collect();
                                            if values.is_empty() {
                                                None
                                            } else {
                                                Some(values)
                                            }
                                        }
                                        _ => None,
                                    })
                                    .map(|child_ids| (id, child_ids))
                            })
                            .unzip();
                        let ids = IdList::try_from_iter(&self.parent_type, ids.into_iter())?;
                        let child_ids = child_ids
                            .into_iter()
                            .map(|ids| IdList::try_from_iter(&self.child_type, ids.into_iter()))
                            .collect::<Result<Vec<_>, _>>()?;
                        (ids, ParentLink::List(child_ids))
                    }
                };
                Ok((
                    ids,
                    EntityLink::Parent(self.parent_type.clone(), parent_link),
                ))
            }
        }
    }
}

/// Encapsulate how we should join a list of parent entities with a list of
/// child entities.
#[derive(Debug)]
struct Join<'a> {
    /// The object type of the child entities
    child_type: ObjectOrInterface<'a>,
    conds: Vec<JoinCond<'a>>,
}

impl<'a> Join<'a> {
    /// Construct a `Join` based on the parent field pointing to the child
    fn new(
        schema: &'a InputSchema,
        parent_type: &'a s::ObjectType,
        child_type: ObjectOrInterface<'a>,
        field_name: &str,
    ) -> Self {
        let child_types = child_type
            .object_types(schema.schema())
            .expect("the name of the child type is valid");

        let conds = child_types
            .iter()
            .map(|child_type| JoinCond::new(schema, parent_type, child_type, field_name))
            .collect();

        Join { child_type, conds }
    }

    fn windows(
        &self,
        schema: &InputSchema,
        parents: &[&mut Node],
        multiplicity: ChildMultiplicity,
        previous_collection: &EntityCollection,
    ) -> Result<Vec<EntityWindow>, QueryExecutionError> {
        let mut windows = vec![];
        let column_names_map = previous_collection.entity_types_and_column_names();
        for cond in &self.conds {
            let mut parents_by_id = parents
                .iter()
                .filter(|parent| parent.typename() == cond.parent_type.as_str())
                .filter_map(|parent| parent.id(schema).ok().map(|id| (id, &**parent)))
                .collect::<Vec<_>>();

            if !parents_by_id.is_empty() {
                parents_by_id.sort_unstable_by(|(id1, _), (id2, _)| id1.cmp(id2));
                parents_by_id.dedup_by(|(id1, _), (id2, _)| id1 == id2);

                let (ids, link) = cond.entity_link(parents_by_id, multiplicity)?;
                let child_type: EntityType = cond.child_type.clone();
                let column_names = match column_names_map.get(&child_type) {
                    Some(column_names) => column_names.clone(),
                    None => AttributeNames::All,
                };
                windows.push(EntityWindow {
                    child_type,
                    ids,
                    link,
                    column_names,
                });
            }
        }
        Ok(windows)
    }
}

/// Distinguish between a root GraphQL query and nested queries. For root
/// queries, there is no parent type, and it doesn't really make sense to
/// worry about join conditions since there is only one parent (the root).
/// In particular, the parent type for root queries is `Query` which is not
/// an entity type, and we would create a `Join` with a fake entity type for
/// the parent type
enum MaybeJoin<'a> {
    Root { child_type: ObjectOrInterface<'a> },
    Nested(Join<'a>),
}

impl<'a> MaybeJoin<'a> {
    fn child_type(&self) -> ObjectOrInterface<'_> {
        match self {
            MaybeJoin::Root { child_type } => child_type.clone(),
            MaybeJoin::Nested(Join {
                child_type,
                conds: _,
            }) => child_type.clone(),
        }
    }
}

/// Link children to their parents. The child nodes are distributed into the
/// parent nodes according to the `parent_id` returned by the database in
/// each child as attribute `g$parent_id`, and are stored in the
/// `response_key` entry in each parent's `children` map.
///
/// The `children` must contain the nodes in the correct order for each
/// parent; we simply pick out matching children for each parent but
/// otherwise maintain the order in `children`
///
/// If `parents` only has one entry, add all children to that one parent. In
/// particular, this is what happens for toplevel queries.
fn add_children(
    schema: &InputSchema,
    parents: &mut [&mut Node],
    children: Vec<Node>,
    response_key: &str,
) -> Result<(), QueryExecutionError> {
    let children: Vec<_> = children.into_iter().map(Rc::new).collect();

    if parents.len() == 1 {
        let parent = parents.first_mut().expect("we just checked");
        parent.set_children(response_key.to_owned(), children);
        return Ok(());
    }

    // Build a map parent_id -> Vec<child> that we will use to add
    // children to their parent. This relies on the fact that interfaces
    // make sure that id's are distinct across all implementations of the
    // interface.
    let mut grouped: HashMap<&Id, Vec<Rc<Node>>> = HashMap::default();
    for child in children.iter() {
        let parent = child.parent.as_ref().ok_or_else(|| {
            QueryExecutionError::Panic(format!(
                "child {}[{}] is missing a parent id",
                child.typename(),
                child
                    .id(schema)
                    .map(|id| id.to_string())
                    .unwrap_or_else(|_| "<no id>".to_owned())
            ))
        })?;
        grouped.entry(parent).or_default().push(child.clone());
    }

    // Add appropriate children using grouped map
    for parent in parents {
        // Set the `response_key` field in `parent`. Make sure that even if `parent` has no
        // matching `children`, the field gets set (to an empty `Vec`).
        //
        // This `insert` will overwrite in the case where the response key occurs both at the
        // interface level and in nested object type conditions. The values for the interface
        // query are always joined first, and may then be overwritten by the merged selection
        // set under the object type condition. See also: e0d6da3e-60cf-41a5-b83c-b60a7a766d4a
        let values = parent
            .id(schema)
            .ok()
            .and_then(|id| grouped.get(&id).cloned());
        parent.set_children(response_key.to_owned(), values.unwrap_or_default());
    }

    Ok(())
}

/// Run the query in `ctx` in such a manner that we only perform one query
/// per 'level' in the query. A query like `musicians { id bands { id } }`
/// will perform two queries: one for musicians, and one for bands, regardless
/// of how many musicians there are.
///
/// The returned value contains a `q::Value::Object` that contains a tree of
/// all the entities (converted into objects) in the form in which they need
/// to be returned. Nested object fields appear under the key `r:response_key`
/// in these objects, and are always `q::Value::List` of objects.
///
/// For the above example, the returned object would have one entry under
/// `r:musicians`, which is a list of all the musicians; each musician has an
/// entry `r:bands` that contains a list of the bands for that musician. Note
/// that even for single-object fields, we return a list so that we can spot
/// cases where the store contains data that violates the data model by having
/// multiple values for what should be a relationship to a single object in
/// @derivedFrom fields
pub fn run(
    resolver: &StoreResolver,
    ctx: &ExecutionContext<impl Resolver>,
    selection_set: &a::SelectionSet,
    graphql_metrics: &GraphQLMetrics,
) -> Result<(r::Value, Trace), Vec<QueryExecutionError>> {
    execute_root_selection_set(resolver, ctx, selection_set).map(|(nodes, trace)| {
        graphql_metrics.observe_query_result_size(nodes.weight());
        let obj = Object::from_iter(nodes.into_iter().flat_map(|node| {
            node.children.into_iter().map(|(key, nodes)| {
                (
                    Word::from(format!("prefetch:{}", key)),
                    node_list_as_value(nodes),
                )
            })
        }));
        (r::Value::Object(obj), trace)
    })
}

/// Executes the root selection set of a query.
fn execute_root_selection_set(
    resolver: &StoreResolver,
    ctx: &ExecutionContext<impl Resolver>,
    selection_set: &a::SelectionSet,
) -> Result<(Vec<Node>, Trace), Vec<QueryExecutionError>> {
    let trace = Trace::root(
        &ctx.query.query_text,
        &ctx.query.variables_text,
        &ctx.query.query_id,
        resolver.block_number(),
        ctx.trace,
    );
    // Execute the root selection set against the root query type
    execute_selection_set(resolver, ctx, make_root_node(), trace, selection_set)
}

fn check_result_size<'a>(
    ctx: &'a ExecutionContext<impl Resolver>,
    size: usize,
) -> Result<(), QueryExecutionError> {
    if size > ENV_VARS.graphql.warn_result_size {
        warn!(ctx.logger, "Large query result"; "size" => size, "query_id" => &ctx.query.query_id);
    }
    if size > ENV_VARS.graphql.error_result_size {
        return Err(QueryExecutionError::ResultTooBig(
            size,
            ENV_VARS.graphql.error_result_size,
        ));
    }
    Ok(())
}

fn execute_selection_set<'a>(
    resolver: &StoreResolver,
    ctx: &'a ExecutionContext<impl Resolver>,
    mut parents: Vec<Node>,
    mut parent_trace: Trace,
    selection_set: &a::SelectionSet,
) -> Result<(Vec<Node>, Trace), Vec<QueryExecutionError>> {
    let schema = &ctx.query.schema;
    let input_schema = resolver.store.input_schema()?;
    let mut errors: Vec<QueryExecutionError> = Vec::new();
    let at_root = is_root_node(parents.iter());

    // Process all field groups in order
    for (object_type, fields) in selection_set.interior_fields() {
        if let Some(deadline) = ctx.deadline {
            if deadline < Instant::now() {
                errors.push(QueryExecutionError::Timeout);
                break;
            }
        }

        // Filter out parents that do not match the type condition.
        let mut parents: Vec<&mut Node> = if at_root {
            parents.iter_mut().collect()
        } else {
            parents
                .iter_mut()
                .filter(|p| object_type.name == p.typename())
                .collect()
        };

        if parents.is_empty() {
            continue;
        }

        for field in fields {
            let field_type = object_type
                .field(&field.name)
                .expect("field names are valid");
            let child_type = schema
                .object_or_interface(field_type.field_type.get_base_type())
                .expect("we only collect fields that are objects or interfaces");

            let join = if at_root {
                MaybeJoin::Root { child_type }
            } else {
                MaybeJoin::Nested(Join::new(
                    &input_schema,
                    object_type,
                    child_type,
                    &field.name,
                ))
            };

            // "Select by Specific Attribute Names" is an experimental feature and can be disabled completely.
            // If this environment variable is set, the program will use an empty collection that,
            // effectively, causes the `AttributeNames::All` variant to be used as a fallback value for all
            // queries.
            let collected_columns = if !ENV_VARS.enable_select_by_specific_attributes {
                SelectedAttributes(BTreeMap::new())
            } else {
                SelectedAttributes::for_field(field)?
            };

            match execute_field(
                resolver,
                ctx,
                &parents,
                &join,
                field,
                field_type,
                collected_columns,
            ) {
                Ok((children, trace)) => {
                    match execute_selection_set(
                        resolver,
                        ctx,
                        children,
                        trace,
                        &field.selection_set,
                    ) {
                        Ok((children, trace)) => {
                            add_children(
                                &input_schema,
                                &mut parents,
                                children,
                                field.response_key(),
                            )?;
                            let weight =
                                parents.iter().map(|parent| parent.weight()).sum::<usize>();
                            check_result_size(ctx, weight)?;
                            parent_trace.push(field.response_key(), trace);
                        }
                        Err(mut e) => errors.append(&mut e),
                    }
                }
                Err(mut e) => {
                    errors.append(&mut e);
                }
            };
        }
    }

    if errors.is_empty() {
        Ok((parents, parent_trace))
    } else {
        Err(errors)
    }
}

/// Executes a field.
fn execute_field(
    resolver: &StoreResolver,
    ctx: &ExecutionContext<impl Resolver>,
    parents: &[&mut Node],
    join: &MaybeJoin<'_>,
    field: &a::Field,
    field_definition: &s::Field,
    selected_attrs: SelectedAttributes,
) -> Result<(Vec<Node>, Trace), Vec<QueryExecutionError>> {
    let multiplicity = if sast::is_list_or_non_null_list_field(field_definition) {
        ChildMultiplicity::Many
    } else {
        ChildMultiplicity::Single
    };

    fetch(
        resolver,
        ctx,
        parents,
        join,
        field,
        multiplicity,
        selected_attrs,
    )
    .map_err(|e| vec![e])
}

/// Query child entities for `parents` from the store. The `join` indicates
/// in which child field to look for the parent's id/join field. When
/// `is_single` is `true`, there is at most one child per parent.
fn fetch(
    resolver: &StoreResolver,
    ctx: &ExecutionContext<impl Resolver>,
    parents: &[&mut Node],
    join: &MaybeJoin<'_>,
    field: &a::Field,
    multiplicity: ChildMultiplicity,
    selected_attrs: SelectedAttributes,
) -> Result<(Vec<Node>, Trace), QueryExecutionError> {
    let input_schema = resolver.store.input_schema()?;
    let mut query = build_query(
        join.child_type(),
        resolver.block_number(),
        field,
        ctx.query.schema.types_for_interface(),
        ctx.max_first,
        ctx.max_skip,
        selected_attrs,
        &super::query::SchemaPair {
            api: ctx.query.schema.clone(),
            input: input_schema.cheap_clone(),
        },
    )?;
    query.trace = ctx.trace;
    query.query_id = Some(ctx.query.query_id.clone());

    if multiplicity == ChildMultiplicity::Single {
        // Suppress 'order by' in lookups of scalar values since
        // that causes unnecessary work in the database
        query.order = EntityOrder::Unordered;
    }

    query.logger = Some(ctx.logger.cheap_clone());
    if let Some(r::Value::String(id)) = field.argument_value(ARG_ID) {
        query.filter = Some(
            EntityFilter::Equal(ARG_ID.to_owned(), StoreValue::from(id.clone()))
                .and_maybe(query.filter),
        );
    }

    if let MaybeJoin::Nested(join) = join {
        // For anything but the root node, restrict the children we select
        // by the parent list
        let windows = join.windows(&input_schema, parents, multiplicity, &query.collection)?;
        if windows.is_empty() {
            return Ok((vec![], Trace::None));
        }
        query.collection = EntityCollection::Window(windows);
    }
    resolver
        .store
        .find_query_values(query)
        .map(|(values, trace)| (values.into_iter().map(Node::from).collect(), trace))
}

#[derive(Debug, Default, Clone)]
pub(crate) struct SelectedAttributes(BTreeMap<String, AttributeNames>);

impl SelectedAttributes {
    /// Extract the attributes we should select from `selection_set`. In
    /// particular, disregard derived fields since they are not stored
    fn for_field(field: &a::Field) -> Result<SelectedAttributes, Vec<QueryExecutionError>> {
        let mut map = BTreeMap::new();
        for (object_type, fields) in field.selection_set.fields() {
            let column_names = fields
                .filter(|field| {
                    // Keep fields that are not derived and for which we
                    // can find the field type
                    sast::get_field(object_type, &field.name)
                        .map(|field_type| !field_type.is_derived())
                        .unwrap_or(false)
                })
                .filter_map(|field| {
                    if field.name.starts_with("__") {
                        None
                    } else {
                        Some(field.name.clone())
                    }
                })
                .collect();
            map.insert(
                object_type.name().to_string(),
                AttributeNames::Select(column_names),
            );
        }
        // We need to also select the `orderBy` field if there is one.
        // Because of how the API Schema is set up, `orderBy` can only have
        // an enum value
        match field.argument_value("orderBy") {
            None => { /* nothing to do */ }
            Some(r::Value::Enum(e)) => {
                for columns in map.values_mut() {
                    columns.add_str(e);
                }
            }
            Some(v) => {
                return Err(vec![constraint_violation!(
                    "'orderBy' attribute must be an enum but is {:?}",
                    v
                )
                .into()]);
            }
        }
        Ok(SelectedAttributes(map))
    }

    pub fn get(&mut self, obj_type: &s::ObjectType) -> AttributeNames {
        self.0.remove(&obj_type.name).unwrap_or(AttributeNames::All)
    }
}
