use graphql_parser::Pos;
use hex::FromHexError;
use serde::ser::*;
use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::string::FromUtf8Error;
use std::sync::Arc;

use crate::data::subgraph::*;
use crate::prelude::q;
use crate::{components::store::StoreError, prelude::CacheWeight};

#[derive(Debug, Clone)]
pub struct CloneableAnyhowError(Arc<anyhow::Error>);

impl From<anyhow::Error> for CloneableAnyhowError {
    fn from(f: anyhow::Error) -> Self {
        Self(Arc::new(f))
    }
}

/// Error caused while executing a [Query](struct.Query.html).
#[derive(Debug, Clone)]
pub enum QueryExecutionError {
    OperationNameRequired,
    OperationNotFound(String),
    NotSupported(String),
    NoRootSubscriptionObjectType,
    NonNullError(Pos, String),
    ListValueError(Pos, String),
    NamedTypeError(String),
    AbstractTypeError(String),
    InvalidArgumentError(Pos, String, q::Value),
    MissingArgumentError(Pos, String),
    ValidationError(Option<Pos>, String),
    InvalidVariableTypeError(Pos, String),
    MissingVariableError(Pos, String),
    ResolveEntitiesError(String),
    OrderByNotSupportedError(String, String),
    OrderByNotSupportedForType(String),
    FilterNotSupportedError(String, String),
    UnknownField(Pos, String, String),
    EmptyQuery,
    MultipleSubscriptionFields,
    SubgraphDeploymentIdError(String),
    RangeArgumentsError(&'static str, u32, i64),
    InvalidFilterError,
    EntityFieldError(String, String),
    ListTypesError(String, Vec<String>),
    ListFilterError(String),
    ChildFilterNestingNotSupportedError(String, String),
    ValueParseError(String, String),
    AttributeTypeError(String, String),
    EntityParseError(String),
    StoreError(CloneableAnyhowError),
    Timeout,
    EmptySelectionSet(String),
    AmbiguousDerivedFromResult(Pos, String, String, String),
    Unimplemented(String),
    EnumCoercionError(Pos, String, q::Value, String, Vec<String>),
    ScalarCoercionError(Pos, String, q::Value, String),
    TooComplex(u64, u64), // (complexity, max_complexity)
    TooDeep(u8),          // max_depth
    CyclicalFragment(String),
    TooExpensive,
    Throttled,
    UndefinedFragment(String),
    Panic(String),
    EventStreamError,
    FulltextQueryRequiresFilter,
    FulltextQueryInvalidSyntax(String),
    DeploymentReverted,
    SubgraphManifestResolveError(Arc<SubgraphManifestResolveError>),
    InvalidSubgraphManifest,
    ResultTooBig(usize, usize),
    DeploymentNotFound(String),
    IdMissing,
    IdNotString,
}

impl QueryExecutionError {
    pub fn is_attestable(&self) -> bool {
        use self::QueryExecutionError::*;
        match self {
            OperationNameRequired
            | OperationNotFound(_)
            | NotSupported(_)
            | NoRootSubscriptionObjectType
            | NonNullError(_, _)
            | NamedTypeError(_)
            | AbstractTypeError(_)
            | InvalidArgumentError(_, _, _)
            | MissingArgumentError(_, _)
            | InvalidVariableTypeError(_, _)
            | MissingVariableError(_, _)
            | OrderByNotSupportedError(_, _)
            | OrderByNotSupportedForType(_)
            | FilterNotSupportedError(_, _)
            | ChildFilterNestingNotSupportedError(_, _)
            | UnknownField(_, _, _)
            | EmptyQuery
            | MultipleSubscriptionFields
            | SubgraphDeploymentIdError(_)
            | InvalidFilterError
            | EntityFieldError(_, _)
            | ListTypesError(_, _)
            | ListFilterError(_)
            | ValueParseError(_, _)
            | AttributeTypeError(_, _)
            | EmptySelectionSet(_)
            | Unimplemented(_)
            | EnumCoercionError(_, _, _, _, _)
            | ScalarCoercionError(_, _, _, _)
            | CyclicalFragment(_)
            | UndefinedFragment(_)
            | FulltextQueryInvalidSyntax(_)
            | FulltextQueryRequiresFilter => true,
            ListValueError(_, _)
            | ResolveEntitiesError(_)
            | RangeArgumentsError(_, _, _)
            | EntityParseError(_)
            | StoreError(_)
            | Timeout
            | AmbiguousDerivedFromResult(_, _, _, _)
            | TooComplex(_, _)
            | TooDeep(_)
            | Panic(_)
            | EventStreamError
            | TooExpensive
            | Throttled
            | DeploymentReverted
            | SubgraphManifestResolveError(_)
            | InvalidSubgraphManifest
            | ValidationError(_, _)
            | ResultTooBig(_, _)
            | DeploymentNotFound(_)
            | IdMissing
            | IdNotString => false,
        }
    }
}

impl Error for QueryExecutionError {
    fn description(&self) -> &str {
        "Query execution error"
    }

    fn cause(&self) -> Option<&dyn Error> {
        None
    }
}

impl fmt::Display for QueryExecutionError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::QueryExecutionError::*;

        match self {
            OperationNameRequired => write!(f, "Operation name required"),
            OperationNotFound(s) => {
                write!(f, "Operation name not found `{}`", s)
            }
            ValidationError(_pos, message) => {
                write!(f, "{}", message)
            }
            NotSupported(s) => write!(f, "Not supported: {}", s),
            NoRootSubscriptionObjectType => {
                write!(f, "No root Subscription type defined in the schema")
            }
            NonNullError(_, s) => {
                write!(f, "Null value resolved for non-null field `{}`", s)
            }
            ListValueError(_, s) => {
                write!(f, "Non-list value resolved for list field `{}`", s)
            }
            NamedTypeError(s) => {
                write!(f, "Failed to resolve named type `{}`", s)
            }
            AbstractTypeError(s) => {
                write!(f, "Failed to resolve abstract type `{}`", s)
            }
            InvalidArgumentError(_, s, v) => {
                write!(f, "Invalid value provided for argument `{}`: {:?}", s, v)
            }
            MissingArgumentError(_, s) => {
                write!(f, "No value provided for required argument: `{}`", s)
            }
            InvalidVariableTypeError(_, s) => {
                write!(f, "Variable `{}` must have an input type", s)
            }
            MissingVariableError(_, s) => {
                write!(f, "No value provided for required variable `{}`", s)
            }
            ResolveEntitiesError(e) => {
                write!(f, "Failed to get entities from store: {}", e)
            }
            OrderByNotSupportedError(entity, field) => {
                write!(f, "Ordering by `{}` is not supported for type `{}`", field, entity)
            }
            OrderByNotSupportedForType(field_type) => {
                write!(f, "Ordering by `{}` fields is not supported", field_type)
            }
            FilterNotSupportedError(value, filter) => {
                write!(f, "Filter not supported by value `{}`: `{}`", value, filter)
            }
            ChildFilterNestingNotSupportedError(value, filter) => {
                write!(f, "Child filter nesting not supported by value `{}`: `{}`", value, filter)
            }
            UnknownField(_, t, s) => {
                write!(f, "Type `{}` has no field `{}`", t, s)
            }
            EmptyQuery => write!(f, "The query is empty"),
            MultipleSubscriptionFields => write!(
                f,
                "Only a single top-level field is allowed in subscriptions"
            ),
            SubgraphDeploymentIdError(s) => {
                write!(f, "Failed to get subgraph ID from type: `{}`", s)
            }
            RangeArgumentsError(arg, max, actual) => {
                write!(f, "The `{}` argument must be between 0 and {}, but is {}", arg, max, actual)
            }
            InvalidFilterError => write!(f, "Filter must by an object"),
            EntityFieldError(e, a) => {
                write!(f, "Entity `{}` has no attribute `{}`", e, a)
            }

            ListTypesError(s, v) => write!(
                f,
                "Values passed to filter `{}` must be of the same type but are of different types: {}",
                s,
                v.join(", ")
            ),
            ListFilterError(s) => {
                write!(f, "Non-list value passed to `{}` filter", s)
            }
            ValueParseError(t, e) => {
                write!(f, "Failed to decode `{}` value: `{}`", t, e)
            }
            AttributeTypeError(value, ty) => {
                write!(f, "Query contains value with invalid type `{}`: `{}`", ty, value)
            }
            EntityParseError(s) => {
                write!(f, "Broken entity found in store: {}", s)
            }
            StoreError(e) => {
                write!(f, "Store error: {}", e.0)
            }
            Timeout => write!(f, "Query timed out"),
            EmptySelectionSet(entity_type) => {
                write!(f, "Selection set for type `{}` is empty", entity_type)
            }
            AmbiguousDerivedFromResult(_, field, target_type, target_field) => {
                write!(f, "Ambiguous result for derived field `{}`: \
                           Multiple `{}` entities refer back via `{}`",
                       field, target_type, target_field)
            }
            Unimplemented(feature) => {
                write!(f, "Feature `{}` is not yet implemented", feature)
            }
            EnumCoercionError(_, field, value, enum_type, values) => {
                write!(f, "Failed to coerce value `{}` of field `{}` to enum type `{}`. Possible values are: {}", value, field, enum_type, values.join(", "))
            }
            ScalarCoercionError(_, field, value, scalar_type) => {
                write!(f, "Failed to coerce value `{}` of field `{}` to scalar type `{}`", value, field, scalar_type)
            }
            TooComplex(complexity, max_complexity) => {
                write!(f, "query potentially returns `{}` entities or more and thereby exceeds \
                           the limit of `{}` entities. Possible solutions are reducing the depth \
                           of the query, querying fewer relationships or using `first` to \
                           return smaller collections", complexity, max_complexity)
            }
            TooDeep(max_depth) => write!(f, "query has a depth that exceeds the limit of `{}`", max_depth),
            CyclicalFragment(name) =>write!(f, "query has fragment cycle including `{}`", name),
            UndefinedFragment(frag_name) => write!(f, "fragment `{}` is not defined", frag_name),
            Panic(msg) => write!(f, "panic processing query: {}", msg),
            EventStreamError => write!(f, "error in the subscription event stream"),
            FulltextQueryRequiresFilter => write!(f, "fulltext search queries can only use EntityFilter::Equal"),
            FulltextQueryInvalidSyntax(msg) => write!(f, "Invalid fulltext search query syntax. Error: {}. Hint: Search terms with spaces need to be enclosed in single quotes", msg),
            TooExpensive => write!(f, "query is too expensive"),
            Throttled => write!(f, "service is overloaded and can not run the query right now. Please try again in a few minutes"),
            DeploymentReverted => write!(f, "the chain was reorganized while executing the query"),
            SubgraphManifestResolveError(e) => write!(f, "failed to resolve subgraph manifest: {}", e),
            InvalidSubgraphManifest => write!(f, "invalid subgraph manifest file"),
            ResultTooBig(actual, limit) => write!(f, "the result size of {} is larger than the allowed limit of {}", actual, limit),
            DeploymentNotFound(id_or_name) => write!(f, "deployment `{}` does not exist", id_or_name),
            IdMissing => write!(f, "entity is missing an `id` attribute"),
            IdNotString => write!(f, "entity `id` attribute is not a string"),
        }
    }
}

impl From<QueryExecutionError> for Vec<QueryExecutionError> {
    fn from(e: QueryExecutionError) -> Self {
        vec![e]
    }
}

impl From<FromHexError> for QueryExecutionError {
    fn from(e: FromHexError) -> Self {
        QueryExecutionError::ValueParseError("Bytes".to_string(), e.to_string())
    }
}

impl From<bigdecimal::ParseBigDecimalError> for QueryExecutionError {
    fn from(e: bigdecimal::ParseBigDecimalError) -> Self {
        QueryExecutionError::ValueParseError("BigDecimal".to_string(), format!("{}", e))
    }
}

impl From<StoreError> for QueryExecutionError {
    fn from(e: StoreError) -> Self {
        match e {
            StoreError::DeploymentNotFound(id_or_name) => {
                QueryExecutionError::DeploymentNotFound(id_or_name)
            }
            StoreError::ChildFilterNestingNotSupportedError(attr, filter) => {
                QueryExecutionError::ChildFilterNestingNotSupportedError(attr, filter)
            }
            _ => QueryExecutionError::StoreError(CloneableAnyhowError(Arc::new(e.into()))),
        }
    }
}

impl From<SubgraphManifestResolveError> for QueryExecutionError {
    fn from(e: SubgraphManifestResolveError) -> Self {
        QueryExecutionError::SubgraphManifestResolveError(Arc::new(e))
    }
}

impl From<anyhow::Error> for QueryExecutionError {
    fn from(e: anyhow::Error) -> Self {
        QueryExecutionError::Panic(e.to_string())
    }
}

impl From<QueryExecutionError> for diesel::result::Error {
    fn from(e: QueryExecutionError) -> Self {
        diesel::result::Error::QueryBuilderError(Box::new(e))
    }
}

/// Error caused while processing a [Query](struct.Query.html) request.
#[derive(Clone, Debug)]
pub enum QueryError {
    EncodingError(FromUtf8Error),
    ParseError(Arc<anyhow::Error>),
    ExecutionError(QueryExecutionError),
    IndexingError,
}

impl QueryError {
    pub fn is_attestable(&self) -> bool {
        match self {
            QueryError::EncodingError(_) | QueryError::ParseError(_) => true,
            QueryError::ExecutionError(err) => err.is_attestable(),
            QueryError::IndexingError => false,
        }
    }
}

impl From<FromUtf8Error> for QueryError {
    fn from(e: FromUtf8Error) -> Self {
        QueryError::EncodingError(e)
    }
}

impl From<QueryExecutionError> for QueryError {
    fn from(e: QueryExecutionError) -> Self {
        QueryError::ExecutionError(e)
    }
}

impl Error for QueryError {
    fn description(&self) -> &str {
        "Query error"
    }

    fn cause(&self) -> Option<&dyn Error> {
        match *self {
            QueryError::EncodingError(ref e) => Some(e),
            QueryError::ExecutionError(ref e) => Some(e),
            _ => None,
        }
    }
}

impl fmt::Display for QueryError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            QueryError::EncodingError(ref e) => write!(f, "{}", e),
            QueryError::ExecutionError(ref e) => write!(f, "{}", e),
            QueryError::ParseError(ref e) => write!(f, "{}", e),

            // This error message is part of attestable responses.
            QueryError::IndexingError => write!(f, "indexing_error"),
        }
    }
}

impl Serialize for QueryError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use self::QueryExecutionError::*;

        let mut map = serializer.serialize_map(Some(1))?;

        let msg = match self {
            // Serialize parse errors with their location (line, column) to make it easier
            // for users to find where the errors are; this is likely to change as the
            // graphql_parser team makes improvements to their error reporting
            QueryError::ParseError(_) => {
                // Split the inner message into (first line, rest)
                let msg = format!("{}", self);
                let inner_msg = msg.replace("query parse error:", "");
                let inner_msg = inner_msg.trim();
                let parts: Vec<&str> = inner_msg.splitn(2, '\n').collect();

                // Find the colon in the first line and split there
                let colon_pos = parts[0].rfind(':').unwrap();
                let (a, b) = parts[0].split_at(colon_pos);

                // Find the line and column numbers and convert them to u32
                let line: u32 = a
                    .matches(char::is_numeric)
                    .collect::<String>()
                    .parse()
                    .unwrap();
                let column: u32 = b
                    .matches(char::is_numeric)
                    .collect::<String>()
                    .parse()
                    .unwrap();

                // Generate the list of locations
                let mut location = HashMap::new();
                location.insert("line", line);
                location.insert("column", column);
                map.serialize_entry("locations", &vec![location])?;

                // Only use the remainder after the location as the error message
                parts[1].to_string()
            }

            // Serialize entity resolution errors using their position
            QueryError::ExecutionError(NonNullError(pos, _))
            | QueryError::ExecutionError(ListValueError(pos, _))
            | QueryError::ExecutionError(InvalidArgumentError(pos, _, _))
            | QueryError::ExecutionError(MissingArgumentError(pos, _))
            | QueryError::ExecutionError(InvalidVariableTypeError(pos, _))
            | QueryError::ExecutionError(MissingVariableError(pos, _))
            | QueryError::ExecutionError(AmbiguousDerivedFromResult(pos, _, _, _))
            | QueryError::ExecutionError(EnumCoercionError(pos, _, _, _, _))
            | QueryError::ExecutionError(ScalarCoercionError(pos, _, _, _))
            | QueryError::ExecutionError(UnknownField(pos, _, _)) => {
                let mut location = HashMap::new();
                location.insert("line", pos.line);
                location.insert("column", pos.column);
                map.serialize_entry("locations", &vec![location])?;
                format!("{}", self)
            }
            _ => format!("{}", self),
        };

        map.serialize_entry("message", msg.as_str())?;
        map.end()
    }
}

impl CacheWeight for QueryError {
    fn indirect_weight(&self) -> usize {
        // Errors don't have a weight since they are never cached
        0
    }
}
