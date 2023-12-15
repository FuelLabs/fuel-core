use graph::data::graphql::DocumentExt as _;
use graph::data::value::{Object, Word};
use graph::schema::ApiSchema;
use graphql_parser::Pos;
use graphql_tools::validation::rules::*;
use graphql_tools::validation::validate::{validate, ValidationPlan};
use lazy_static::lazy_static;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::iter::FromIterator;
use std::sync::Arc;
use std::time::Instant;
use std::{collections::hash_map::DefaultHasher, convert::TryFrom};

use graph::data::graphql::{ext::TypeExt, ObjectOrInterface};
use graph::data::query::QueryExecutionError;
use graph::data::query::{Query as GraphDataQuery, QueryVariables};
use graph::prelude::{
    info, o, q, r, s, warn, BlockNumber, CheapClone, DeploymentHash, EntityRange, GraphQLMetrics,
    Logger, TryFromValue, ENV_VARS,
};
use graph::schema::ast::{self as sast};
use graph::schema::ErrorPolicy;

use crate::execution::ast as a;
use crate::execution::get_field;
use crate::query::{ast as qast, ext::BlockConstraint};
use crate::values::coercion;

lazy_static! {
    static ref GRAPHQL_VALIDATION_PLAN: ValidationPlan =
        ValidationPlan::from(if !ENV_VARS.graphql.enable_validations {
            vec![]
        } else {
            vec![
                Box::new(UniqueOperationNames::new()),
                Box::new(LoneAnonymousOperation::new()),
                Box::new(SingleFieldSubscriptions::new()),
                Box::new(KnownTypeNames::new()),
                Box::new(FragmentsOnCompositeTypes::new()),
                Box::new(VariablesAreInputTypes::new()),
                Box::new(LeafFieldSelections::new()),
                Box::new(FieldsOnCorrectType::new()),
                Box::new(UniqueFragmentNames::new()),
                Box::new(KnownFragmentNames::new()),
                Box::new(NoUnusedFragments::new()),
                Box::new(OverlappingFieldsCanBeMerged::new()),
                Box::new(NoFragmentsCycle::new()),
                Box::new(PossibleFragmentSpreads::new()),
                Box::new(NoUnusedVariables::new()),
                Box::new(NoUndefinedVariables::new()),
                Box::new(KnownArgumentNames::new()),
                Box::new(UniqueArgumentNames::new()),
                Box::new(UniqueVariableNames::new()),
                Box::new(ProvidedRequiredArguments::new()),
                Box::new(KnownDirectives::new()),
                Box::new(VariablesInAllowedPosition::new()),
                Box::new(ValuesOfCorrectType::new()),
                Box::new(UniqueDirectivesPerLocation::new()),
            ]
        });
}

#[derive(Clone, Debug)]
pub enum ComplexityError {
    TooDeep,
    Overflow,
    Invalid,
    CyclicalFragment(String),
}

#[derive(Copy, Clone)]
enum Kind {
    Query,
    Subscription,
}

/// Helper to log the fields in a `SelectionSet` without cloning. Writes
/// a list of field names from the selection set separated by ';'. Using
/// ';' as a separator makes parsing the log a little easier since slog
/// uses ',' to separate key/value pairs.
/// If `SelectionSet` is `None`, log `*` to indicate that the query was
/// for the entire selection set of the query
struct SelectedFields<'a>(&'a a::SelectionSet);

impl<'a> std::fmt::Display for SelectedFields<'a> {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        let mut first = true;
        for (obj_type, fields) in self.0.fields() {
            write!(fmt, "{}:", obj_type.name)?;
            for field in fields {
                if first {
                    write!(fmt, "{}", field.response_key())?;
                } else {
                    write!(fmt, ";{}", field.response_key())?;
                }
                first = false;
            }
        }
        if first {
            // There wasn't a single `q::Selection::Field` in the set. That
            // seems impossible, but log '-' to be on the safe side
            write!(fmt, "-")?;
        }

        Ok(())
    }
}

/// A GraphQL query that has been preprocessed and checked and is ready
/// for execution. Checking includes validating all query fields and, if
/// desired, checking the query's complexity
//
// The implementation contains various workarounds to make it compatible
// with the previous implementation when it comes to queries that are not
// fully spec compliant and should be rejected through rigorous validation
// against the GraphQL spec. Once we do validate queries, code that is
// marked with `graphql-bug-compat` can be deleted.
pub struct Query {
    /// The schema against which to execute the query
    pub schema: Arc<ApiSchema>,
    /// The root selection set of the query. All variable references have already been resolved
    pub selection_set: Arc<a::SelectionSet>,
    /// The ShapeHash of the original query
    pub shape_hash: u64,

    pub network: Option<String>,

    pub logger: Logger,

    start: Instant,

    kind: Kind,

    /// Used only for logging; if logging is configured off, these will
    /// have dummy values
    pub query_text: Arc<String>,
    pub variables_text: Arc<String>,
    pub query_id: String,
}

fn validate_query(
    logger: &Logger,
    query: &GraphDataQuery,
    document: &s::Document,
    metrics: &Arc<dyn GraphQLMetrics>,
    id: &DeploymentHash,
) -> Result<(), Vec<QueryExecutionError>> {
    let validation_errors = validate(document, &query.document, &GRAPHQL_VALIDATION_PLAN);

    if !validation_errors.is_empty() {
        if !ENV_VARS.graphql.silent_graphql_validations {
            return Err(validation_errors
                .into_iter()
                .map(|e| {
                    QueryExecutionError::ValidationError(
                        e.locations.first().cloned(),
                        e.message.clone(),
                    )
                })
                .collect());
        } else {
            warn!(
              &logger,
              "GraphQL Validation failure";
              "query" => &query.query_text,
              "variables" => &query.variables_text,
              "errors" => format!("[{:?}]", validation_errors.iter().map(|e| e.message.clone()).collect::<Vec<_>>().join(", "))
            );

            let error_codes = validation_errors
                .iter()
                .map(|e| e.error_code)
                .collect::<Vec<_>>();

            metrics.observe_query_validation_error(error_codes, id);
        }
    }

    Ok(())
}

impl Query {
    /// Process the raw GraphQL query `query` and prepare for executing it.
    /// The returned `Query` has already been validated and, if `max_complexity`
    /// is given, also checked whether it is too complex. If validation fails,
    /// or the query is too complex, errors are returned
    pub fn new(
        logger: &Logger,
        schema: Arc<ApiSchema>,
        network: Option<String>,
        query: GraphDataQuery,
        max_complexity: Option<u64>,
        max_depth: u8,
        metrics: Arc<dyn GraphQLMetrics>,
    ) -> Result<Arc<Self>, Vec<QueryExecutionError>> {
        let query_hash = {
            let mut hasher = DefaultHasher::new();
            query.query_text.hash(&mut hasher);
            query.variables_text.hash(&mut hasher);
            hasher.finish()
        };
        let query_id = format!("{:x}-{:x}", query.shape_hash, query_hash);
        let logger = logger.new(o!(
            "subgraph_id" => schema.id().clone(),
            "query_id" => query_id.clone()
        ));

        let validation_phase_start = Instant::now();
        validate_query(&logger, &query, schema.document(), &metrics, schema.id())?;
        metrics.observe_query_validation(validation_phase_start.elapsed(), schema.id());

        let mut operation = None;
        let mut fragments = HashMap::new();
        for defn in query.document.definitions.into_iter() {
            match defn {
                q::Definition::Operation(op) => match operation {
                    None => operation = Some(op),
                    Some(_) => return Err(vec![QueryExecutionError::OperationNameRequired]),
                },
                q::Definition::Fragment(frag) => {
                    fragments.insert(frag.name.clone(), frag);
                }
            }
        }
        let operation = operation.ok_or(QueryExecutionError::OperationNameRequired)?;

        let variables = coerce_variables(schema.as_ref(), &operation, query.variables)?;
        let (kind, selection_set) = match operation {
            q::OperationDefinition::Query(q::Query { selection_set, .. }) => {
                (Kind::Query, selection_set)
            }
            // Queries can be run by just sending a selection set
            q::OperationDefinition::SelectionSet(selection_set) => (Kind::Query, selection_set),
            q::OperationDefinition::Subscription(q::Subscription { selection_set, .. }) => {
                (Kind::Subscription, selection_set)
            }
            q::OperationDefinition::Mutation(_) => {
                return Err(vec![QueryExecutionError::NotSupported(
                    "Mutations are not supported".to_owned(),
                )])
            }
        };

        let start = Instant::now();
        let root_type = match kind {
            Kind::Query => schema.query_type.as_ref(),
            Kind::Subscription => schema.subscription_type.as_ref().unwrap(),
        };
        // Use an intermediate struct so we can modify the query before
        // enclosing it in an Arc
        let raw_query = RawQuery {
            schema: schema.cheap_clone(),
            variables,
            selection_set,
            fragments,
            root_type,
        };

        // It's important to check complexity first, so `validate_fields`
        // doesn't risk a stack overflow from invalid queries. We don't
        // really care about the resulting complexity, only that all the
        // checks that `check_complexity` performs pass successfully
        let _ = raw_query.check_complexity(max_complexity, max_depth)?;
        raw_query.validate_fields()?;
        let selection_set = raw_query.convert()?;

        let query = Self {
            schema,
            selection_set: Arc::new(selection_set),
            shape_hash: query.shape_hash,
            kind,
            network,
            logger,
            start,
            query_text: query.query_text.cheap_clone(),
            variables_text: query.variables_text.cheap_clone(),
            query_id,
        };

        Ok(Arc::new(query))
    }

    /// Return the block constraint for the toplevel query field(s), merging
    /// consecutive fields that have the same block constraint, while making
    /// sure that the fields appear in the same order as they did in the
    /// query
    ///
    /// Also returns the combined error policy for those fields, which is
    /// `Deny` if any field is `Deny` and `Allow` otherwise.
    pub fn block_constraint(
        &self,
    ) -> Result<Vec<(BlockConstraint, (a::SelectionSet, ErrorPolicy))>, Vec<QueryExecutionError>>
    {
        let mut bcs: Vec<(BlockConstraint, (a::SelectionSet, ErrorPolicy))> = Vec::new();

        let root_type = sast::ObjectType::from(self.schema.query_type.cheap_clone());
        let mut prev_bc: Option<BlockConstraint> = None;
        for field in self.selection_set.fields_for(&root_type)? {
            let bc = match field.argument_value("block") {
                Some(bc) => BlockConstraint::try_from_value(bc).map_err(|_| {
                    vec![QueryExecutionError::InvalidArgumentError(
                        Pos::default(),
                        "block".to_string(),
                        bc.clone().into(),
                    )]
                })?,
                None => BlockConstraint::Latest,
            };

            let field_error_policy = match field.argument_value("subgraphError") {
                Some(value) => ErrorPolicy::try_from(value).map_err(|_| {
                    vec![QueryExecutionError::InvalidArgumentError(
                        Pos::default(),
                        "subgraphError".to_string(),
                        value.clone().into(),
                    )]
                })?,
                None => ErrorPolicy::Deny,
            };

            let next_bc = Some(bc.clone());
            if prev_bc == next_bc {
                let (selection_set, error_policy) = &mut bcs.last_mut().unwrap().1;
                selection_set.push(field)?;
                if field_error_policy == ErrorPolicy::Deny {
                    *error_policy = ErrorPolicy::Deny;
                }
            } else {
                let mut selection_set = a::SelectionSet::empty_from(&self.selection_set);
                selection_set.push(field)?;
                bcs.push((bc, (selection_set, field_error_policy)))
            }
            prev_bc = next_bc;
        }
        Ok(bcs)
    }

    /// Return `true` if this is a query, and not a subscription or
    /// mutation
    pub fn is_query(&self) -> bool {
        match self.kind {
            Kind::Query => true,
            Kind::Subscription => false,
        }
    }

    /// Return `true` if this is a subscription, not a query or a mutation
    pub fn is_subscription(&self) -> bool {
        match self.kind {
            Kind::Subscription => true,
            Kind::Query => false,
        }
    }

    /// Log details about the overall execution of the query
    pub fn log_execution(&self, block: BlockNumber) {
        if ENV_VARS.log_gql_timing() {
            info!(
                &self.logger,
                "Query timing (GraphQL)";
                "query" => &self.query_text,
                "variables" => &self.variables_text,
                "query_time_ms" => self.start.elapsed().as_millis(),
                "block" => block,
            );
        }
    }

    /// Log details about how the part of the query corresponding to
    /// `selection_set` was cached
    pub fn log_cache_status(
        &self,
        selection_set: &a::SelectionSet,
        block: BlockNumber,
        start: Instant,
        cache_status: String,
    ) {
        if ENV_VARS.log_gql_cache_timing() {
            info!(
                &self.logger,
                "Query caching";
                "query_time_ms" => start.elapsed().as_millis(),
                "cached" => cache_status,
                "selection" => %SelectedFields(selection_set),
                "block" => block,
            );
        }
    }
}

/// Coerces variable values for an operation.
pub fn coerce_variables(
    schema: &ApiSchema,
    operation: &q::OperationDefinition,
    mut variables: Option<QueryVariables>,
) -> Result<HashMap<String, r::Value>, Vec<QueryExecutionError>> {
    let mut coerced_values = HashMap::new();
    let mut errors = vec![];

    for variable_def in qast::get_variable_definitions(operation)
        .into_iter()
        .flatten()
    {
        // Skip variable if it has an invalid type
        if !schema.is_input_type(&variable_def.var_type) {
            errors.push(QueryExecutionError::InvalidVariableTypeError(
                variable_def.position,
                variable_def.name.clone(),
            ));
            continue;
        }

        let value = variables
            .as_mut()
            .and_then(|vars| vars.remove(&variable_def.name));

        let value = match value.or_else(|| {
            variable_def
                .default_value
                .clone()
                .map(r::Value::try_from)
                .transpose()
                .unwrap()
        }) {
            // No variable value provided and no default for non-null type, fail
            None => {
                if sast::is_non_null_type(&variable_def.var_type) {
                    errors.push(QueryExecutionError::MissingVariableError(
                        variable_def.position,
                        variable_def.name.clone(),
                    ));
                };
                continue;
            }
            Some(value) => value,
        };

        // We have a variable value, attempt to coerce it to the value type
        // of the variable definition
        coerced_values.insert(
            variable_def.name.clone(),
            coerce_variable(schema, variable_def, value)?,
        );
    }

    if errors.is_empty() {
        Ok(coerced_values)
    } else {
        Err(errors)
    }
}

fn coerce_variable(
    schema: &ApiSchema,
    variable_def: &q::VariableDefinition,
    value: r::Value,
) -> Result<r::Value, Vec<QueryExecutionError>> {
    use crate::values::coercion::coerce_value;

    let resolver = |name: &str| schema.get_named_type(name);

    coerce_value(value, &variable_def.var_type, &resolver).map_err(|value| {
        vec![QueryExecutionError::InvalidArgumentError(
            variable_def.position,
            variable_def.name.clone(),
            value.into(),
        )]
    })
}

struct RawQuery<'s> {
    /// The schema against which to execute the query
    schema: Arc<ApiSchema>,
    /// The variables for the query, coerced into proper values
    variables: HashMap<String, r::Value>,
    /// The root selection set of the query
    selection_set: q::SelectionSet,

    fragments: HashMap<String, q::FragmentDefinition>,
    root_type: &'s s::ObjectType,
}

impl<'s> RawQuery<'s> {
    fn check_complexity(
        &self,
        max_complexity: Option<u64>,
        max_depth: u8,
    ) -> Result<u64, Vec<QueryExecutionError>> {
        let complexity = self.complexity(max_depth).map_err(|e| vec![e])?;
        if let Some(max_complexity) = max_complexity {
            if complexity > max_complexity {
                return Err(vec![QueryExecutionError::TooComplex(
                    complexity,
                    max_complexity,
                )]);
            }
        }
        Ok(complexity)
    }

    fn complexity_inner<'a>(
        &'a self,
        ty: &s::TypeDefinition,
        selection_set: &'a q::SelectionSet,
        max_depth: u8,
        depth: u8,
        visited_fragments: &'a HashSet<&'a str>,
    ) -> Result<u64, ComplexityError> {
        use ComplexityError::*;

        if depth >= max_depth {
            return Err(TooDeep);
        }

        selection_set
            .items
            .iter()
            .try_fold(0, |total_complexity, selection| {
                match selection {
                    q::Selection::Field(field) => {
                        // Empty selection sets are the base case.
                        if field.selection_set.items.is_empty() {
                            return Ok(total_complexity);
                        }

                        // Get field type to determine if this is a collection query.
                        let s_field = match ty {
                            s::TypeDefinition::Object(t) => get_field(t, &field.name),
                            s::TypeDefinition::Interface(t) => get_field(t, &field.name),

                            // `Scalar` and `Enum` cannot have selection sets.
                            // `InputObject` can't appear in a selection.
                            // `Union` is not yet supported.
                            s::TypeDefinition::Scalar(_)
                            | s::TypeDefinition::Enum(_)
                            | s::TypeDefinition::InputObject(_)
                            | s::TypeDefinition::Union(_) => None,
                        }
                        .ok_or(Invalid)?;

                        let field_complexity = self.complexity_inner(
                            self.schema
                                .get_named_type(s_field.field_type.get_base_type())
                                .ok_or(Invalid)?,
                            &field.selection_set,
                            max_depth,
                            depth + 1,
                            visited_fragments,
                        )?;

                        // Non-collection queries pass through.
                        if !sast::is_list_or_non_null_list_field(&s_field) {
                            return Ok(total_complexity + field_complexity);
                        }

                        // For collection queries, check the `first` argument.
                        let max_entities = qast::get_argument_value(&field.arguments, "first")
                            .and_then(|arg| match arg {
                                q::Value::Int(n) => Some(n.as_i64()? as u64),
                                _ => None,
                            })
                            .unwrap_or(EntityRange::FIRST as u64);
                        max_entities
                            .checked_add(
                                max_entities.checked_mul(field_complexity).ok_or(Overflow)?,
                            )
                            .ok_or(Overflow)
                    }
                    q::Selection::FragmentSpread(fragment) => {
                        let def = self.fragments.get(&fragment.fragment_name).unwrap();
                        let q::TypeCondition::On(type_name) = &def.type_condition;
                        let ty = self.schema.get_named_type(type_name).ok_or(Invalid)?;

                        // Copy `visited_fragments` on write.
                        let mut visited_fragments = visited_fragments.clone();
                        if !visited_fragments.insert(&fragment.fragment_name) {
                            return Err(CyclicalFragment(fragment.fragment_name.clone()));
                        }
                        self.complexity_inner(
                            ty,
                            &def.selection_set,
                            max_depth,
                            depth + 1,
                            &visited_fragments,
                        )
                    }
                    q::Selection::InlineFragment(fragment) => {
                        let ty = match &fragment.type_condition {
                            Some(q::TypeCondition::On(type_name)) => {
                                self.schema.get_named_type(type_name).ok_or(Invalid)?
                            }
                            _ => ty,
                        };
                        self.complexity_inner(
                            ty,
                            &fragment.selection_set,
                            max_depth,
                            depth + 1,
                            visited_fragments,
                        )
                    }
                }
                .and_then(|complexity| total_complexity.checked_add(complexity).ok_or(Overflow))
            })
    }

    /// See https://developer.github.com/v4/guides/resource-limitations/.
    ///
    /// If the query is invalid, returns `Ok(0)` so that execution proceeds and
    /// gives a proper error.
    fn complexity(&self, max_depth: u8) -> Result<u64, QueryExecutionError> {
        let root_type = self.schema.get_root_query_type_def().unwrap();

        match self.complexity_inner(
            root_type,
            &self.selection_set,
            max_depth,
            0,
            &HashSet::new(),
        ) {
            Ok(complexity) => Ok(complexity),
            Err(ComplexityError::Invalid) => Ok(0),
            Err(ComplexityError::TooDeep) => Err(QueryExecutionError::TooDeep(max_depth)),
            Err(ComplexityError::Overflow) => {
                Err(QueryExecutionError::TooComplex(u64::max_value(), 0))
            }
            Err(ComplexityError::CyclicalFragment(name)) => {
                Err(QueryExecutionError::CyclicalFragment(name))
            }
        }
    }

    fn validate_fields(&self) -> Result<(), Vec<QueryExecutionError>> {
        let root_type = self.schema.query_type.as_ref();

        let errors = self.validate_fields_inner("Query", root_type.into(), &self.selection_set);
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    // Checks for invalid selections.
    fn validate_fields_inner(
        &self,
        type_name: &str,
        ty: ObjectOrInterface<'_>,
        selection_set: &q::SelectionSet,
    ) -> Vec<QueryExecutionError> {
        selection_set
            .items
            .iter()
            .fold(vec![], |mut errors, selection| {
                match selection {
                    q::Selection::Field(field) => match get_field(ty, &field.name) {
                        Some(s_field) => {
                            let base_type = s_field.field_type.get_base_type();
                            if self.schema.get_named_type(base_type).is_none() {
                                errors.push(QueryExecutionError::NamedTypeError(base_type.into()));
                            } else if let Some(ty) = self.schema.object_or_interface(base_type) {
                                errors.extend(self.validate_fields_inner(
                                    base_type,
                                    ty,
                                    &field.selection_set,
                                ))
                            }
                        }
                        None => errors.push(QueryExecutionError::UnknownField(
                            field.position,
                            type_name.into(),
                            field.name.clone(),
                        )),
                    },
                    q::Selection::FragmentSpread(fragment) => {
                        match self.fragments.get(&fragment.fragment_name) {
                            Some(frag) => {
                                let q::TypeCondition::On(type_name) = &frag.type_condition;
                                match self.schema.object_or_interface(type_name) {
                                    Some(ty) => errors.extend(self.validate_fields_inner(
                                        type_name,
                                        ty,
                                        &frag.selection_set,
                                    )),
                                    None => errors.push(QueryExecutionError::NamedTypeError(
                                        type_name.clone(),
                                    )),
                                }
                            }
                            None => errors.push(QueryExecutionError::UndefinedFragment(
                                fragment.fragment_name.clone(),
                            )),
                        }
                    }
                    q::Selection::InlineFragment(fragment) => match &fragment.type_condition {
                        Some(q::TypeCondition::On(type_name)) => {
                            match self.schema.object_or_interface(type_name) {
                                Some(ty) => errors.extend(self.validate_fields_inner(
                                    type_name,
                                    ty,
                                    &fragment.selection_set,
                                )),
                                None => errors
                                    .push(QueryExecutionError::NamedTypeError(type_name.clone())),
                            }
                        }
                        _ => errors.extend(self.validate_fields_inner(
                            type_name,
                            ty,
                            &fragment.selection_set,
                        )),
                    },
                }
                errors
            })
    }

    fn convert(self) -> Result<a::SelectionSet, Vec<QueryExecutionError>> {
        let RawQuery {
            schema,
            variables,
            selection_set,
            fragments,
            root_type,
        } = self;

        let transform = Transform {
            schema,
            variables,
            fragments,
        };
        transform.expand_selection_set(selection_set, &a::ObjectTypeSet::Any, root_type.into())
    }
}

struct Transform {
    schema: Arc<ApiSchema>,
    variables: HashMap<String, r::Value>,
    fragments: HashMap<String, q::FragmentDefinition>,
}

impl Transform {
    /// Look up the value of the variable `name`. If the variable is not
    /// defined, return `r::Value::Null`
    // graphql-bug-compat: Once queries are fully validated, all variables
    // will be defined
    fn variable(&self, name: &str) -> r::Value {
        self.variables.get(name).cloned().unwrap_or(r::Value::Null)
    }

    /// Interpolate variable references in the arguments `args`
    fn interpolate_arguments(
        &self,
        args: Vec<(String, q::Value)>,
        pos: &Pos,
    ) -> Vec<(String, r::Value)> {
        args.into_iter()
            .map(|(name, val)| {
                let val = self.interpolate_value(val, pos);
                (name, val)
            })
            .collect()
    }

    /// Turn `value` into an `r::Value` by resolving variable references
    fn interpolate_value(&self, value: q::Value, pos: &Pos) -> r::Value {
        match value {
            q::Value::Variable(var) => self.variable(&var),
            q::Value::Int(ref num) => {
                r::Value::Int(num.as_i64().expect("q::Value::Int contains an i64"))
            }
            q::Value::Float(f) => r::Value::Float(f),
            q::Value::String(s) => r::Value::String(s),
            q::Value::Boolean(b) => r::Value::Boolean(b),
            q::Value::Null => r::Value::Null,
            q::Value::Enum(s) => r::Value::Enum(s),
            q::Value::List(vals) => {
                let vals = vals
                    .into_iter()
                    .map(|val| self.interpolate_value(val, pos))
                    .collect();
                r::Value::List(vals)
            }
            q::Value::Object(map) => {
                let mut rmap = BTreeMap::new();
                for (key, value) in map.into_iter() {
                    let value = self.interpolate_value(value, pos);
                    rmap.insert(key.into(), value);
                }
                r::Value::object(rmap)
            }
        }
    }

    /// Interpolate variable references in directives. Return the directives
    /// and a boolean indicating whether the element these directives are
    /// attached to should be skipped
    fn interpolate_directives(
        &self,
        dirs: Vec<q::Directive>,
    ) -> Result<(Vec<a::Directive>, bool), QueryExecutionError> {
        let dirs: Vec<_> = dirs
            .into_iter()
            .map(|dir| {
                let q::Directive {
                    name,
                    position,
                    arguments,
                } = dir;
                let arguments = self.interpolate_arguments(arguments, &position);
                a::Directive {
                    name,
                    position,
                    arguments,
                }
            })
            .collect();
        let skip = dirs.iter().any(|dir| dir.skip());
        Ok((dirs, skip))
    }

    /// Coerces argument values into GraphQL values.
    pub fn coerce_argument_values<'a>(
        &self,
        arguments: &mut Vec<(String, r::Value)>,
        ty: ObjectOrInterface<'a>,
        field_name: &str,
    ) -> Result<(), Vec<QueryExecutionError>> {
        let mut errors = vec![];

        let resolver = |name: &str| self.schema.get_named_type(name);

        let mut defined_args: usize = 0;
        for argument_def in sast::get_argument_definitions(ty, field_name)
            .into_iter()
            .flatten()
        {
            let arg_value = arguments
                .iter_mut()
                .find(|arg| arg.0 == argument_def.name)
                .map(|arg| &mut arg.1);
            if arg_value.is_some() {
                defined_args += 1;
            }
            match coercion::coerce_input_value(
                arg_value.as_deref().cloned(),
                argument_def,
                &resolver,
            ) {
                Ok(Some(value)) => {
                    let value = if argument_def.name == *"text" {
                        r::Value::Object(Object::from_iter(vec![(Word::from(field_name), value)]))
                    } else {
                        value
                    };
                    match arg_value {
                        Some(arg_value) => *arg_value = value,
                        None => arguments.push((argument_def.name.clone(), value)),
                    }
                }
                Ok(None) => {}
                Err(e) => errors.push(e),
            }
        }

        // see: graphql-bug-compat
        // avoids error 'unknown argument on field'
        if defined_args < arguments.len() {
            // `arguments` contains undefined arguments, remove them
            match sast::get_argument_definitions(ty, field_name) {
                None => arguments.clear(),
                Some(arg_defs) => {
                    arguments.retain(|(name, _)| arg_defs.iter().any(|def| &def.name == name))
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Expand fragments and interpolate variables in a field. Return `None`
    /// if the field should be skipped
    fn expand_field(
        &self,
        field: q::Field,
        parent_type: ObjectOrInterface<'_>,
    ) -> Result<Option<a::Field>, Vec<QueryExecutionError>> {
        let q::Field {
            position,
            alias,
            name,
            arguments,
            directives,
            selection_set,
        } = field;

        // Short-circuit '__typename' since it is not a real field
        if name == "__typename" {
            return Ok(Some(a::Field {
                position,
                alias,
                name,
                arguments: vec![],
                directives: vec![],
                selection_set: a::SelectionSet::new(vec![]),
            }));
        }

        let field_type = parent_type.field(&name).ok_or_else(|| {
            vec![QueryExecutionError::UnknownField(
                position,
                parent_type.name().to_string(),
                name.clone(),
            )]
        })?;

        let (directives, skip) = self.interpolate_directives(directives)?;
        if skip {
            return Ok(None);
        }

        let mut arguments = self.interpolate_arguments(arguments, &position);
        self.coerce_argument_values(&mut arguments, parent_type, &name)?;

        let is_leaf_type = self.schema.document().is_leaf_type(&field_type.field_type);
        let selection_set = if selection_set.items.is_empty() {
            if !is_leaf_type {
                // see: graphql-bug-compat
                // Field requires selection, ignore this field
                return Ok(None);
            }
            a::SelectionSet::new(vec![])
        } else if is_leaf_type {
            // see: graphql-bug-compat
            // Field does not allow selections, ignore selections
            a::SelectionSet::new(vec![])
        } else {
            let ty = field_type.field_type.get_base_type();
            let type_set = a::ObjectTypeSet::from_name(&self.schema, ty)?;
            let ty = self.schema.object_or_interface(ty).unwrap();
            self.expand_selection_set(selection_set, &type_set, ty)?
        };

        Ok(Some(a::Field {
            position,
            alias,
            name,
            arguments,
            directives,
            selection_set,
        }))
    }

    /// Expand fragments and interpolate variables in a selection set
    fn expand_selection_set(
        &self,
        set: q::SelectionSet,
        type_set: &a::ObjectTypeSet,
        ty: ObjectOrInterface<'_>,
    ) -> Result<a::SelectionSet, Vec<QueryExecutionError>> {
        let q::SelectionSet { span: _, items } = set;
        // check_complexity already checked for cycles in fragment
        // expansion, i.e. situations where a named fragment includes itself
        // recursively. We still want to guard against spreading the same
        // fragment twice at the same level in the query
        let mut visited_fragments = HashSet::new();

        // All the types that could possibly be returned by this selection set
        let types = type_set.type_names(&self.schema, ty)?;
        let mut newset = a::SelectionSet::new(types);

        for sel in items {
            match sel {
                q::Selection::Field(field) => {
                    if let Some(field) = self.expand_field(field, ty)? {
                        newset.push(&field)?;
                    }
                }
                q::Selection::FragmentSpread(spread) => {
                    // TODO: we ignore the directives here (and so did the
                    // old implementation), but that seems wrong
                    let q::FragmentSpread {
                        position: _,
                        fragment_name,
                        directives: _,
                    } = spread;
                    let frag = self.fragments.get(&fragment_name).unwrap();
                    if visited_fragments.insert(fragment_name) {
                        let q::FragmentDefinition {
                            position: _,
                            name: _,
                            type_condition,
                            directives,
                            selection_set,
                        } = frag;
                        self.expand_fragment(
                            directives.clone(),
                            Some(type_condition),
                            type_set,
                            selection_set.clone(),
                            ty,
                            &mut newset,
                        )?;
                    }
                }
                q::Selection::InlineFragment(frag) => {
                    let q::InlineFragment {
                        position: _,
                        type_condition,
                        directives,
                        selection_set,
                    } = frag;
                    self.expand_fragment(
                        directives,
                        type_condition.as_ref(),
                        type_set,
                        selection_set,
                        ty,
                        &mut newset,
                    )?;
                }
            }
        }
        Ok(newset)
    }

    fn expand_fragment(
        &self,
        directives: Vec<q::Directive>,
        frag_cond: Option<&q::TypeCondition>,
        type_set: &a::ObjectTypeSet,
        selection_set: q::SelectionSet,
        ty: ObjectOrInterface,
        newset: &mut a::SelectionSet,
    ) -> Result<(), Vec<QueryExecutionError>> {
        let (directives, skip) = self.interpolate_directives(directives)?;
        // Field names in fragment spreads refer to this type, which will
        // usually be different from the outer type
        let ty = match frag_cond {
            Some(q::TypeCondition::On(name)) => self
                .schema
                .object_or_interface(name)
                .expect("type names on fragment spreads are valid"),
            None => ty,
        };
        if !skip {
            let type_set = a::ObjectTypeSet::convert(&self.schema, frag_cond)?.intersect(type_set);
            let selection_set = self.expand_selection_set(selection_set, &type_set, ty)?;
            newset.merge(selection_set, directives)?;
        }
        Ok(())
    }
}
