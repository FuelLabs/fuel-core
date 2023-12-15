use std::collections::{BTreeMap, HashMap};
use std::str::FromStr;

use graphql_parser::Pos;
use inflector::Inflector;
use lazy_static::lazy_static;

use crate::data::graphql::{ObjectOrInterface, ObjectTypeExt};
use crate::data::store::IdType;
use crate::schema::{ast, META_FIELD_NAME, META_FIELD_TYPE};

use crate::data::graphql::ext::{DefinitionExt, DirectiveExt, DocumentExt, ValueExt};
use crate::prelude::s::{Value, *};
use crate::prelude::*;
use thiserror::Error;

use super::{Schema, SCHEMA_TYPE_NAME};

#[derive(Error, Debug)]
pub enum APISchemaError {
    #[error("type {0} already exists in the input schema")]
    TypeExists(String),
    #[error("Type {0} not found")]
    TypeNotFound(String),
    #[error("Fulltext search is not yet deterministic")]
    FulltextSearchNonDeterministic,
    #[error("Illegal type for `id`: {0}")]
    IllegalIdType(String),
}

// The followoing types are defined in meta.graphql
const BLOCK_HEIGHT: &str = "Block_height";
const CHANGE_BLOCK_FILTER_NAME: &str = "BlockChangedFilter";
const ERROR_POLICY_TYPE: &str = "_SubgraphErrorPolicy_";

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum ErrorPolicy {
    Allow,
    Deny,
}

impl std::str::FromStr for ErrorPolicy {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<ErrorPolicy, anyhow::Error> {
        match s {
            "allow" => Ok(ErrorPolicy::Allow),
            "deny" => Ok(ErrorPolicy::Deny),
            _ => Err(anyhow::anyhow!("failed to parse `{}` as ErrorPolicy", s)),
        }
    }
}

impl TryFrom<&q::Value> for ErrorPolicy {
    type Error = anyhow::Error;

    /// `value` should be the output of input value coercion.
    fn try_from(value: &q::Value) -> Result<Self, Self::Error> {
        match value {
            q::Value::Enum(s) => ErrorPolicy::from_str(s),
            _ => Err(anyhow::anyhow!("invalid `ErrorPolicy`")),
        }
    }
}

impl TryFrom<&r::Value> for ErrorPolicy {
    type Error = anyhow::Error;

    /// `value` should be the output of input value coercion.
    fn try_from(value: &r::Value) -> Result<Self, Self::Error> {
        match value {
            r::Value::Enum(s) => ErrorPolicy::from_str(s),
            _ => Err(anyhow::anyhow!("invalid `ErrorPolicy`")),
        }
    }
}

/// A GraphQL schema used for responding to queries. These schemas can be
/// generated in one of two ways:
///
/// (1) By calling `api_schema()` on an `InputSchema`. This is the way to
/// generate a query schema for a subgraph.
///
/// (2) By parsing an appropriate GraphQL schema from text and calling
/// `from_graphql_schema`. In that case, it's the caller's responsibility to
/// make sure that the schema has all the types needed for querying, like
/// `Query` and `Subscription`
///
/// Because of the second point, once constructed, it can not be assumed
/// that an `ApiSchema` is based on an `InputSchema` and it can only be used
/// for querying.
#[derive(Debug)]
pub struct ApiSchema {
    schema: Schema,

    // Root types for the api schema.
    pub query_type: Arc<ObjectType>,
    pub subscription_type: Option<Arc<ObjectType>>,
    object_types: HashMap<String, Arc<ObjectType>>,
}

impl ApiSchema {
    /// Set up the `ApiSchema`, mostly by extracting important pieces of
    /// information from it like `query_type` etc.
    ///
    /// In addition, the API schema has an introspection schema mixed into
    /// `api_schema`. In particular, the `Query` type has fields called
    /// `__schema` and `__type`
    pub(in crate::schema) fn from_api_schema(mut schema: Schema) -> Result<Self, anyhow::Error> {
        add_introspection_schema(&mut schema.document);

        let query_type = schema
            .document
            .get_root_query_type()
            .context("no root `Query` in the schema")?
            .clone();
        let subscription_type = schema
            .document
            .get_root_subscription_type()
            .cloned()
            .map(Arc::new);

        let object_types = HashMap::from_iter(
            schema
                .document
                .get_object_type_definitions()
                .into_iter()
                .map(|obj_type| (obj_type.name.clone(), Arc::new(obj_type.clone()))),
        );

        Ok(Self {
            schema,
            query_type: Arc::new(query_type),
            subscription_type,
            object_types,
        })
    }

    /// Create an API Schema that can be used to execute GraphQL queries.
    /// This method is only meant for schemas that are not derived from a
    /// subgraph schema, like the schema for the index-node server. Use
    /// `InputSchema::api_schema` to get an API schema for a subgraph
    pub fn from_graphql_schema(schema: Schema) -> Result<Self, anyhow::Error> {
        Self::from_api_schema(schema)
    }

    pub fn document(&self) -> &s::Document {
        &self.schema.document
    }

    pub fn id(&self) -> &DeploymentHash {
        &self.schema.id
    }

    pub fn schema(&self) -> &Schema {
        &self.schema
    }

    pub fn types_for_interface(&self) -> &BTreeMap<String, Vec<ObjectType>> {
        &self.schema.types_for_interface
    }

    /// Returns `None` if the type implements no interfaces.
    pub fn interfaces_for_type(&self, type_name: &str) -> Option<&Vec<InterfaceType>> {
        self.schema.interfaces_for_type(type_name)
    }

    /// Return an `Arc` around the `ObjectType` from our internal cache
    ///
    /// # Panics
    /// If `obj_type` is not part of this schema, this function panics
    pub fn object_type(&self, obj_type: &ObjectType) -> Arc<ObjectType> {
        self.object_types
            .get(&obj_type.name)
            .expect("ApiSchema.object_type is only used with existing types")
            .cheap_clone()
    }

    pub fn get_named_type(&self, name: &str) -> Option<&s::TypeDefinition> {
        self.schema.document.get_named_type(name)
    }

    /// Returns true if the given type is an input type.
    ///
    /// Uses the algorithm outlined on
    /// https://facebook.github.io/graphql/draft/#IsInputType().
    pub fn is_input_type(&self, t: &s::Type) -> bool {
        match t {
            s::Type::NamedType(name) => {
                let named_type = self.get_named_type(name);
                named_type.map_or(false, |type_def| match type_def {
                    s::TypeDefinition::Scalar(_)
                    | s::TypeDefinition::Enum(_)
                    | s::TypeDefinition::InputObject(_) => true,
                    _ => false,
                })
            }
            s::Type::ListType(inner) => self.is_input_type(inner),
            s::Type::NonNullType(inner) => self.is_input_type(inner),
        }
    }

    pub fn get_root_query_type_def(&self) -> Option<&s::TypeDefinition> {
        self.schema
            .document
            .definitions
            .iter()
            .find_map(|d| match d {
                s::Definition::TypeDefinition(def @ s::TypeDefinition::Object(_)) => match def {
                    s::TypeDefinition::Object(t) if t.name == "Query" => Some(def),
                    _ => None,
                },
                _ => None,
            })
    }

    pub fn object_or_interface(&self, name: &str) -> Option<ObjectOrInterface<'_>> {
        if name.starts_with("__") {
            INTROSPECTION_SCHEMA.object_or_interface(name)
        } else {
            self.schema.document.object_or_interface(name)
        }
    }

    /// Returns the type definition that a field type corresponds to.
    pub fn get_type_definition_from_field<'a>(
        &'a self,
        field: &s::Field,
    ) -> Option<&'a s::TypeDefinition> {
        self.get_type_definition_from_type(&field.field_type)
    }

    /// Returns the type definition for a type.
    pub fn get_type_definition_from_type<'a>(
        &'a self,
        t: &s::Type,
    ) -> Option<&'a s::TypeDefinition> {
        match t {
            s::Type::NamedType(name) => self.get_named_type(name),
            s::Type::ListType(inner) => self.get_type_definition_from_type(inner),
            s::Type::NonNullType(inner) => self.get_type_definition_from_type(inner),
        }
    }

    #[cfg(debug_assertions)]
    pub fn definitions(&self) -> impl Iterator<Item = &s::Definition> {
        self.schema.document.definitions.iter()
    }
}

lazy_static! {
    static ref INTROSPECTION_SCHEMA: Document = {
        let schema = include_str!("introspection.graphql");
        parse_schema(schema).expect("the schema `introspection.graphql` is invalid")
    };
    pub static ref INTROSPECTION_QUERY_TYPE: ast::ObjectType = {
        let root_query_type = INTROSPECTION_SCHEMA
            .get_root_query_type()
            .expect("Schema does not have a root query type");
        ast::ObjectType::from(Arc::new(root_query_type.clone()))
    };
}

pub fn is_introspection_field(name: &str) -> bool {
    INTROSPECTION_QUERY_TYPE.field(name).is_some()
}

/// Extend `schema` with the definitions from the introspection schema and
/// modify the root query type to contain the fields from the introspection
/// schema's root query type.
///
/// This results in a schema that combines the original schema with the
/// introspection schema
fn add_introspection_schema(schema: &mut Document) {
    fn introspection_fields() -> Vec<Field> {
        // Generate fields for the root query fields in an introspection schema,
        // the equivalent of the fields of the `Query` type:
        //
        // type Query {
        //   __schema: __Schema!
        //   __type(name: String!): __Type
        // }

        let type_args = vec![InputValue {
            position: Pos::default(),
            description: None,
            name: "name".to_string(),
            value_type: Type::NonNullType(Box::new(Type::NamedType("String".to_string()))),
            default_value: None,
            directives: vec![],
        }];

        vec![
            Field {
                position: Pos::default(),
                description: None,
                name: "__schema".to_string(),
                arguments: vec![],
                field_type: Type::NonNullType(Box::new(Type::NamedType("__Schema".to_string()))),
                directives: vec![],
            },
            Field {
                position: Pos::default(),
                description: None,
                name: "__type".to_string(),
                arguments: type_args,
                field_type: Type::NamedType("__Type".to_string()),
                directives: vec![],
            },
        ]
    }

    // Add all definitions from the introspection schema to the schema,
    // except for the root query type as that qould clobber the 'real' root
    // query type
    schema.definitions.extend(
        INTROSPECTION_SCHEMA
            .definitions
            .iter()
            .filter(|dfn| !dfn.is_root_query_type())
            .cloned(),
    );

    let query_type = schema
        .definitions
        .iter_mut()
        .filter_map(|d| match d {
            Definition::TypeDefinition(TypeDefinition::Object(t)) if t.name == "Query" => Some(t),
            _ => None,
        })
        .peekable()
        .next()
        .expect("no root `Query` in the schema");
    query_type.fields.append(&mut introspection_fields());
}

/// Derives a full-fledged GraphQL API schema from an input schema.
///
/// The input schema should only have type/enum/interface/union definitions
/// and must not include a root Query type. This Query type is derived, with
/// all its fields and their input arguments, based on the existing types.
pub(in crate::schema) fn api_schema(input_schema: &Schema) -> Result<Document, APISchemaError> {
    // Refactor: Take `input_schema` by value.
    let object_types = input_schema.document.get_object_type_definitions();
    let interface_types = input_schema.document.get_interface_type_definitions();

    // Refactor: Don't clone the schema.
    let mut schema = input_schema.clone();
    add_meta_field_type(&mut schema.document);
    add_types_for_object_types(&mut schema, &object_types)?;
    add_types_for_interface_types(&mut schema, &interface_types)?;
    add_field_arguments(&mut schema.document, &input_schema.document)?;
    add_query_type(&mut schema.document, &object_types, &interface_types)?;
    add_subscription_type(&mut schema.document, &object_types, &interface_types)?;

    // Remove the `_Schema_` type from the generated schema.
    schema.document.definitions.retain(|d| match d {
        Definition::TypeDefinition(def @ TypeDefinition::Object(_)) => match def {
            TypeDefinition::Object(t) if t.name.eq(SCHEMA_TYPE_NAME) => false,
            _ => true,
        },
        _ => true,
    });

    Ok(schema.document)
}

/// Adds a global `_Meta_` type to the schema. The `_meta` field
/// accepts values of this type
fn add_meta_field_type(schema: &mut Document) {
    lazy_static! {
        static ref META_FIELD_SCHEMA: Document = {
            let schema = include_str!("meta.graphql");
            parse_schema(schema).expect("the schema `meta.graphql` is invalid")
        };
    }

    schema
        .definitions
        .extend(META_FIELD_SCHEMA.definitions.iter().cloned());
}

fn add_types_for_object_types(
    schema: &mut Schema,
    object_types: &[&ObjectType],
) -> Result<(), APISchemaError> {
    for object_type in object_types {
        if !object_type.name.eq(SCHEMA_TYPE_NAME) {
            add_order_by_type(&mut schema.document, &object_type.name, &object_type.fields)?;
            add_filter_type(schema, &object_type.name, &object_type.fields)?;
        }
    }
    Ok(())
}

/// Adds `*_orderBy` and `*_filter` enum types for the given interfaces to the schema.
fn add_types_for_interface_types(
    schema: &mut Schema,
    interface_types: &[&InterfaceType],
) -> Result<(), APISchemaError> {
    for interface_type in interface_types {
        add_order_by_type(
            &mut schema.document,
            &interface_type.name,
            &interface_type.fields,
        )?;
        add_filter_type(schema, &interface_type.name, &interface_type.fields)?;
    }
    Ok(())
}

/// Adds a `<type_name>_orderBy` enum type for the given fields to the schema.
fn add_order_by_type(
    schema: &mut Document,
    type_name: &str,
    fields: &[Field],
) -> Result<(), APISchemaError> {
    let type_name = format!("{}_orderBy", type_name);

    match schema.get_named_type(&type_name) {
        None => {
            let typedef = TypeDefinition::Enum(EnumType {
                position: Pos::default(),
                description: None,
                name: type_name,
                directives: vec![],
                values: field_enum_values(schema, fields)?,
            });
            let def = Definition::TypeDefinition(typedef);
            schema.definitions.push(def);
        }
        Some(_) => return Err(APISchemaError::TypeExists(type_name)),
    }
    Ok(())
}

/// Generates enum values for the given set of fields.
fn field_enum_values(
    schema: &Document,
    fields: &[Field],
) -> Result<Vec<EnumValue>, APISchemaError> {
    let mut enum_values = vec![];
    for field in fields {
        enum_values.push(EnumValue {
            position: Pos::default(),
            description: None,
            name: field.name.clone(),
            directives: vec![],
        });
        enum_values.extend(field_enum_values_from_child_entity(schema, field)?);
    }
    Ok(enum_values)
}

fn enum_value_from_child_entity_field(
    schema: &Document,
    parent_field_name: &str,
    field: &Field,
) -> Option<EnumValue> {
    if ast::is_list_or_non_null_list_field(field) || ast::is_entity_type(schema, &field.field_type)
    {
        // Sorting on lists or entities is not supported.
        None
    } else {
        Some(EnumValue {
            position: Pos::default(),
            description: None,
            name: format!("{}__{}", parent_field_name, field.name),
            directives: vec![],
        })
    }
}

fn field_enum_values_from_child_entity(
    schema: &Document,
    field: &Field,
) -> Result<Vec<EnumValue>, APISchemaError> {
    fn resolve_supported_type_name(field_type: &Type) -> Option<&String> {
        match field_type {
            Type::NamedType(name) => Some(name),
            Type::ListType(_) => None,
            Type::NonNullType(of_type) => resolve_supported_type_name(of_type),
        }
    }

    let type_name = match ENV_VARS.graphql.disable_child_sorting {
        true => None,
        false => resolve_supported_type_name(&field.field_type),
    };

    Ok(match type_name {
        Some(name) => {
            let named_type = schema
                .get_named_type(name)
                .ok_or_else(|| APISchemaError::TypeNotFound(name.clone()))?;
            match named_type {
                TypeDefinition::Object(ObjectType { fields, .. })
                | TypeDefinition::Interface(InterfaceType { fields, .. }) => fields
                    .iter()
                    .filter_map(|f| {
                        enum_value_from_child_entity_field(schema, field.name.as_str(), f)
                    })
                    .collect(),
                _ => vec![],
            }
        }
        None => vec![],
    })
}

/// Adds a `<type_name>_filter` enum type for the given fields to the schema.
fn add_filter_type(
    schema: &mut Schema,
    type_name: &str,
    fields: &[Field],
) -> Result<(), APISchemaError> {
    let filter_type_name = format!("{}_filter", type_name);
    match schema.document.get_named_type(&filter_type_name) {
        None => {
            let mut generated_filter_fields = field_input_values(schema, fields)?;
            generated_filter_fields.push(block_changed_filter_argument());

            if !ENV_VARS.graphql.disable_bool_filters {
                generated_filter_fields.push(InputValue {
                    position: Pos::default(),
                    description: None,
                    name: "and".to_string(),
                    value_type: Type::ListType(Box::new(Type::NamedType(filter_type_name.clone()))),
                    default_value: None,
                    directives: vec![],
                });

                generated_filter_fields.push(InputValue {
                    position: Pos::default(),
                    description: None,
                    name: "or".to_string(),
                    value_type: Type::ListType(Box::new(Type::NamedType(filter_type_name.clone()))),
                    default_value: None,
                    directives: vec![],
                });
            }

            let typedef = TypeDefinition::InputObject(InputObjectType {
                position: Pos::default(),
                description: None,
                name: filter_type_name,
                directives: vec![],
                fields: generated_filter_fields,
            });
            let def = Definition::TypeDefinition(typedef);
            schema.document.definitions.push(def);
        }
        Some(_) => return Err(APISchemaError::TypeExists(filter_type_name)),
    }

    Ok(())
}

/// Generates `*_filter` input values for the given set of fields.
fn field_input_values(
    schema: &Schema,
    fields: &[Field],
) -> Result<Vec<InputValue>, APISchemaError> {
    let mut input_values = vec![];
    for field in fields {
        input_values.extend(field_filter_input_values(schema, field, &field.field_type)?);
    }
    Ok(input_values)
}

/// Generates `*_filter` input values for the given field.
fn field_filter_input_values(
    schema: &Schema,
    field: &Field,
    field_type: &Type,
) -> Result<Vec<InputValue>, APISchemaError> {
    match field_type {
        Type::NamedType(ref name) => {
            let named_type = schema
                .document
                .get_named_type(name)
                .ok_or_else(|| APISchemaError::TypeNotFound(name.clone()))?;
            Ok(match named_type {
                TypeDefinition::Object(_) | TypeDefinition::Interface(_) => {
                    let scalar_type = id_type_as_scalar(schema, named_type)?.unwrap();
                    let mut input_values = match ast::get_derived_from_directive(field) {
                        // Only add `where` filter fields for object and interface fields
                        // if they are not @derivedFrom
                        Some(_) => vec![],
                        // We allow filtering with `where: { other: "some-id" }` and
                        // `where: { others: ["some-id", "other-id"] }`. In both cases,
                        // we allow ID strings as the values to be passed to these
                        // filters.
                        None => {
                            field_scalar_filter_input_values(&schema.document, field, &scalar_type)
                        }
                    };
                    extend_with_child_filter_input_value(field, name, &mut input_values);
                    input_values
                }
                TypeDefinition::Scalar(ref t) => {
                    field_scalar_filter_input_values(&schema.document, field, t)
                }
                TypeDefinition::Enum(ref t) => {
                    field_enum_filter_input_values(&schema.document, field, t)
                }
                _ => vec![],
            })
        }
        Type::ListType(ref t) => {
            Ok(field_list_filter_input_values(schema, field, t)?.unwrap_or_default())
        }
        Type::NonNullType(ref t) => field_filter_input_values(schema, field, t),
    }
}

fn id_type_as_scalar(
    schema: &Schema,
    typedef: &TypeDefinition,
) -> Result<Option<ScalarType>, APISchemaError> {
    let id_type = match typedef {
        TypeDefinition::Object(obj_type) => IdType::try_from(obj_type)
            .map(Option::Some)
            .map_err(|_| APISchemaError::IllegalIdType(obj_type.name.to_owned())),
        TypeDefinition::Interface(intf_type) => {
            match schema
                .types_for_interface
                .get(&intf_type.name)
                .and_then(|obj_types| obj_types.first())
            {
                None => Ok(Some(IdType::String)),
                Some(obj_type) => IdType::try_from(obj_type)
                    .map(Option::Some)
                    .map_err(|_| APISchemaError::IllegalIdType(obj_type.name.to_owned())),
            }
        }
        _ => Ok(None),
    }?;
    let scalar_type = id_type.map(|id_type| match id_type {
        IdType::String | IdType::Bytes => ScalarType::new(String::from("String")),
        // It would be more logical to use "Int8" here, but currently, that
        // leads to values being turned into strings, not i64 which causes
        // database queries to fail in various places. Once this is fixed
        // (check e.g., `Value::coerce_scalar` in `graph/src/data/value.rs`)
        // we can turn that into "Int8". For now, queries can only query
        // Int8 id values up to i32::MAX.
        IdType::Int8 => ScalarType::new(String::from("Int")),
    });
    Ok(scalar_type)
}

/// Generates `*_filter` input values for the given scalar field.
fn field_scalar_filter_input_values(
    _schema: &Document,
    field: &Field,
    field_type: &ScalarType,
) -> Vec<InputValue> {
    match field_type.name.as_ref() {
        "BigInt" => vec!["", "not", "gt", "lt", "gte", "lte", "in", "not_in"],
        "Boolean" => vec!["", "not", "in", "not_in"],
        "Bytes" => vec![
            "",
            "not",
            "gt",
            "lt",
            "gte",
            "lte",
            "in",
            "not_in",
            "contains",
            "not_contains",
        ],
        "BigDecimal" => vec!["", "not", "gt", "lt", "gte", "lte", "in", "not_in"],
        "ID" => vec!["", "not", "gt", "lt", "gte", "lte", "in", "not_in"],
        "Int" => vec!["", "not", "gt", "lt", "gte", "lte", "in", "not_in"],
        "Int8" => vec!["", "not", "gt", "lt", "gte", "lte", "in", "not_in"],
        "String" => vec![
            "",
            "not",
            "gt",
            "lt",
            "gte",
            "lte",
            "in",
            "not_in",
            "contains",
            "contains_nocase",
            "not_contains",
            "not_contains_nocase",
            "starts_with",
            "starts_with_nocase",
            "not_starts_with",
            "not_starts_with_nocase",
            "ends_with",
            "ends_with_nocase",
            "not_ends_with",
            "not_ends_with_nocase",
        ],
        _ => vec!["", "not"],
    }
    .into_iter()
    .map(|filter_type| {
        let field_type = Type::NamedType(field_type.name.clone());
        let value_type = match filter_type {
            "in" | "not_in" => Type::ListType(Box::new(Type::NonNullType(Box::new(field_type)))),
            _ => field_type,
        };
        input_value(&field.name, filter_type, value_type)
    })
    .collect()
}

/// Appends a child filter to input values
fn extend_with_child_filter_input_value(
    field: &Field,
    field_type_name: &String,
    input_values: &mut Vec<InputValue>,
) {
    input_values.push(input_value(
        &format!("{}_", field.name),
        "",
        Type::NamedType(format!("{}_filter", field_type_name)),
    ));
}

/// Generates `*_filter` input values for the given enum field.
fn field_enum_filter_input_values(
    _schema: &Document,
    field: &Field,
    field_type: &EnumType,
) -> Vec<InputValue> {
    vec!["", "not", "in", "not_in"]
        .into_iter()
        .map(|filter_type| {
            let field_type = Type::NamedType(field_type.name.clone());
            let value_type = match filter_type {
                "in" | "not_in" => {
                    Type::ListType(Box::new(Type::NonNullType(Box::new(field_type))))
                }
                _ => field_type,
            };
            input_value(&field.name, filter_type, value_type)
        })
        .collect()
}

/// Generates `*_filter` input values for the given list field.
fn field_list_filter_input_values(
    schema: &Schema,
    field: &Field,
    field_type: &Type,
) -> Result<Option<Vec<InputValue>>, APISchemaError> {
    // Only add a filter field if the type of the field exists in the schema
    let typedef = match ast::get_type_definition_from_type(&schema.document, field_type) {
        Some(typedef) => typedef,
        None => return Ok(None),
    };

    // Decide what type of values can be passed to the filter. In the case
    // one-to-many or many-to-many object or interface fields that are not
    // derived, we allow ID strings to be passed on.
    // Adds child filter only to object types.
    let (input_field_type, parent_type_name) = match typedef {
        TypeDefinition::Object(ObjectType { name, .. })
        | TypeDefinition::Interface(InterfaceType { name, .. }) => {
            if ast::get_derived_from_directive(field).is_some() {
                (None, Some(name.clone()))
            } else {
                let scalar_type = id_type_as_scalar(schema, typedef)?.unwrap();
                let named_type = Type::NamedType(scalar_type.name);
                (Some(named_type), Some(name.clone()))
            }
        }
        TypeDefinition::Scalar(ref t) => (Some(Type::NamedType(t.name.clone())), None),
        TypeDefinition::Enum(ref t) => (Some(Type::NamedType(t.name.clone())), None),
        TypeDefinition::InputObject(_) | TypeDefinition::Union(_) => (None, None),
    };

    let mut input_values: Vec<InputValue> = match input_field_type {
        None => {
            vec![]
        }
        Some(input_field_type) => vec![
            "",
            "not",
            "contains",
            "contains_nocase",
            "not_contains",
            "not_contains_nocase",
        ]
        .into_iter()
        .map(|filter_type| {
            input_value(
                &field.name,
                filter_type,
                Type::ListType(Box::new(Type::NonNullType(Box::new(
                    input_field_type.clone(),
                )))),
            )
        })
        .collect(),
    };

    if let Some(parent) = parent_type_name {
        extend_with_child_filter_input_value(field, &parent, &mut input_values);
    }

    Ok(Some(input_values))
}

/// Generates a `*_filter` input value for the given field name, suffix and value type.
fn input_value(name: &str, suffix: &'static str, value_type: Type) -> InputValue {
    InputValue {
        position: Pos::default(),
        description: None,
        name: if suffix.is_empty() {
            name.to_owned()
        } else {
            format!("{}_{}", name, suffix)
        },
        value_type,
        default_value: None,
        directives: vec![],
    }
}

/// Adds a root `Query` object type to the schema.
fn add_query_type(
    schema: &mut Document,
    object_types: &[&ObjectType],
    interface_types: &[&InterfaceType],
) -> Result<(), APISchemaError> {
    let type_name = String::from("Query");

    if schema.get_named_type(&type_name).is_some() {
        return Err(APISchemaError::TypeExists(type_name));
    }

    let mut fields = object_types
        .iter()
        .map(|t| t.name.as_str())
        .filter(|name| !name.eq(&SCHEMA_TYPE_NAME))
        .chain(interface_types.iter().map(|t| t.name.as_str()))
        .flat_map(query_fields_for_type)
        .collect::<Vec<Field>>();
    let mut fulltext_fields = schema
        .get_fulltext_directives()
        .map_err(|_| APISchemaError::FulltextSearchNonDeterministic)?
        .iter()
        .filter_map(|fulltext| query_field_for_fulltext(fulltext))
        .collect();
    fields.append(&mut fulltext_fields);
    fields.push(meta_field());

    let typedef = TypeDefinition::Object(ObjectType {
        position: Pos::default(),
        description: None,
        name: type_name,
        implements_interfaces: vec![],
        directives: vec![],
        fields,
    });
    let def = Definition::TypeDefinition(typedef);
    schema.definitions.push(def);
    Ok(())
}

fn query_field_for_fulltext(fulltext: &Directive) -> Option<Field> {
    let name = fulltext.argument("name").unwrap().as_str().unwrap().into();

    let includes = fulltext.argument("include").unwrap().as_list().unwrap();
    // Only one include is allowed per fulltext directive
    let include = includes.iter().next().unwrap();
    let included_entity = include.as_object().unwrap();
    let entity_name = included_entity.get("entity").unwrap().as_str().unwrap();

    let mut arguments = vec![
        // text: String
        InputValue {
            position: Pos::default(),
            description: None,
            name: String::from("text"),
            value_type: Type::NonNullType(Box::new(Type::NamedType(String::from("String")))),
            default_value: None,
            directives: vec![],
        },
        // first: Int
        InputValue {
            position: Pos::default(),
            description: None,
            name: String::from("first"),
            value_type: Type::NamedType(String::from("Int")),
            default_value: Some(Value::Int(100.into())),
            directives: vec![],
        },
        // skip: Int
        InputValue {
            position: Pos::default(),
            description: None,
            name: String::from("skip"),
            value_type: Type::NamedType(String::from("Int")),
            default_value: Some(Value::Int(0.into())),
            directives: vec![],
        },
        // block: BlockHeight
        block_argument(),
        input_value(
            "where",
            "",
            Type::NamedType(format!("{}_filter", entity_name)),
        ),
    ];

    arguments.push(subgraph_error_argument());

    Some(Field {
        position: Pos::default(),
        description: None,
        name,
        arguments,
        field_type: Type::NonNullType(Box::new(Type::ListType(Box::new(Type::NonNullType(
            Box::new(Type::NamedType(entity_name.into())),
        ))))), // included entity type name
        directives: vec![fulltext.clone()],
    })
}

/// Adds a root `Subscription` object type to the schema.
fn add_subscription_type(
    schema: &mut Document,
    object_types: &[&ObjectType],
    interface_types: &[&InterfaceType],
) -> Result<(), APISchemaError> {
    let type_name = String::from("Subscription");

    if schema.get_named_type(&type_name).is_some() {
        return Err(APISchemaError::TypeExists(type_name));
    }

    let mut fields: Vec<Field> = object_types
        .iter()
        .map(|t| &t.name)
        .filter(|name| !name.eq(&SCHEMA_TYPE_NAME))
        .chain(interface_types.iter().map(|t| &t.name))
        .flat_map(|name| query_fields_for_type(name))
        .collect();
    fields.push(meta_field());

    let typedef = TypeDefinition::Object(ObjectType {
        position: Pos::default(),
        description: None,
        name: type_name,
        implements_interfaces: vec![],
        directives: vec![],
        fields,
    });
    let def = Definition::TypeDefinition(typedef);
    schema.definitions.push(def);
    Ok(())
}

fn block_argument() -> InputValue {
    InputValue {
        position: Pos::default(),
        description: Some(
            "The block at which the query should be executed. \
             Can either be a `{ hash: Bytes }` value containing a block hash, \
             a `{ number: Int }` containing the block number, \
             or a `{ number_gte: Int }` containing the minimum block number. \
             In the case of `number_gte`, the query will be executed on the latest block only if \
             the subgraph has progressed to or past the minimum block number. \
             Defaults to the latest block when omitted."
                .to_owned(),
        ),
        name: "block".to_string(),
        value_type: Type::NamedType(BLOCK_HEIGHT.to_owned()),
        default_value: None,
        directives: vec![],
    }
}

fn block_changed_filter_argument() -> InputValue {
    InputValue {
        position: Pos::default(),
        description: Some("Filter for the block changed event.".to_owned()),
        name: "_change_block".to_string(),
        value_type: Type::NamedType(CHANGE_BLOCK_FILTER_NAME.to_owned()),
        default_value: None,
        directives: vec![],
    }
}

fn subgraph_error_argument() -> InputValue {
    InputValue {
        position: Pos::default(),
        description: Some(
            "Set to `allow` to receive data even if the subgraph has skipped over errors while syncing."
                .to_owned(),
        ),
        name: "subgraphError".to_string(),
        value_type: Type::NonNullType(Box::new(Type::NamedType(ERROR_POLICY_TYPE.to_string()))),
        default_value: Some(Value::Enum("deny".to_string())),
        directives: vec![],
    }
}

/// Generates `Query` fields for the given type name (e.g. `users` and `user`).
fn query_fields_for_type(type_name: &str) -> Vec<Field> {
    let mut collection_arguments = collection_arguments_for_named_type(type_name);
    collection_arguments.push(block_argument());

    let mut by_id_arguments = vec![
        InputValue {
            position: Pos::default(),
            description: None,
            name: "id".to_string(),
            value_type: Type::NonNullType(Box::new(Type::NamedType("ID".to_string()))),
            default_value: None,
            directives: vec![],
        },
        block_argument(),
    ];

    collection_arguments.push(subgraph_error_argument());
    by_id_arguments.push(subgraph_error_argument());

    vec![
        Field {
            position: Pos::default(),
            description: None,
            name: type_name.to_camel_case(), // Name formatting must be updated in sync with `graph::data::schema::validate_fulltext_directive_name()`
            arguments: by_id_arguments,
            field_type: Type::NamedType(type_name.to_owned()),
            directives: vec![],
        },
        Field {
            position: Pos::default(),
            description: None,
            name: type_name.to_plural().to_camel_case(), // Name formatting must be updated in sync with `graph::data::schema::validate_fulltext_directive_name()`
            arguments: collection_arguments,
            field_type: Type::NonNullType(Box::new(Type::ListType(Box::new(Type::NonNullType(
                Box::new(Type::NamedType(type_name.to_owned())),
            ))))),
            directives: vec![],
        },
    ]
}

fn meta_field() -> Field {
    lazy_static! {
        static ref META_FIELD: Field = Field {
            position: Pos::default(),
            description: Some("Access to subgraph metadata".to_string()),
            name: META_FIELD_NAME.to_string(),
            arguments: vec![
                // block: BlockHeight
                InputValue {
                    position: Pos::default(),
                    description: None,
                    name: String::from("block"),
                    value_type: Type::NamedType(BLOCK_HEIGHT.to_string()),
                    default_value: None,
                    directives: vec![],
                },
            ],
            field_type: Type::NamedType(META_FIELD_TYPE.to_string()),
            directives: vec![],
        };
    }
    META_FIELD.clone()
}

/// Generates arguments for collection queries of a named type (e.g. User).
fn collection_arguments_for_named_type(type_name: &str) -> Vec<InputValue> {
    // `first` and `skip` should be non-nullable, but the Apollo graphql client
    // exhibts non-conforming behaviour by erroing if no value is provided for a
    // non-nullable field, regardless of the presence of a default.
    let mut skip = input_value("skip", "", Type::NamedType("Int".to_string()));
    skip.default_value = Some(Value::Int(0.into()));

    let mut first = input_value("first", "", Type::NamedType("Int".to_string()));
    first.default_value = Some(Value::Int(100.into()));

    let args = vec![
        skip,
        first,
        input_value(
            "orderBy",
            "",
            Type::NamedType(format!("{}_orderBy", type_name)),
        ),
        input_value(
            "orderDirection",
            "",
            Type::NamedType("OrderDirection".to_string()),
        ),
        input_value(
            "where",
            "",
            Type::NamedType(format!("{}_filter", type_name)),
        ),
    ];

    args
}

fn add_field_arguments(
    schema: &mut Document,
    input_schema: &Document,
) -> Result<(), APISchemaError> {
    // Refactor: Remove the `input_schema` argument and do a mutable iteration
    // over the definitions in `schema`. Also the duplication between this and
    // the loop for interfaces below.
    for input_object_type in input_schema.get_object_type_definitions() {
        for input_field in &input_object_type.fields {
            if let Some(input_reference_type) =
                ast::get_referenced_entity_type(input_schema, input_field)
            {
                if ast::is_list_or_non_null_list_field(input_field) {
                    // Get corresponding object type and field in the output schema
                    let object_type = ast::get_object_type_mut(schema, &input_object_type.name)
                        .expect("object type from input schema is missing in API schema");
                    let mut field = object_type
                        .fields
                        .iter_mut()
                        .find(|field| field.name == input_field.name)
                        .expect("field from input schema is missing in API schema");

                    match input_reference_type {
                        TypeDefinition::Object(ot) => {
                            field.arguments = collection_arguments_for_named_type(&ot.name);
                        }
                        TypeDefinition::Interface(it) => {
                            field.arguments = collection_arguments_for_named_type(&it.name);
                        }
                        _ => unreachable!(
                            "referenced entity types can only be object or interface types"
                        ),
                    }
                }
            }
        }
    }

    for input_interface_type in input_schema.get_interface_type_definitions() {
        for input_field in &input_interface_type.fields {
            if let Some(input_reference_type) =
                ast::get_referenced_entity_type(input_schema, input_field)
            {
                if ast::is_list_or_non_null_list_field(input_field) {
                    // Get corresponding interface type and field in the output schema
                    let interface_type =
                        ast::get_interface_type_mut(schema, &input_interface_type.name)
                            .expect("interface type from input schema is missing in API schema");
                    let mut field = interface_type
                        .fields
                        .iter_mut()
                        .find(|field| field.name == input_field.name)
                        .expect("field from input schema is missing in API schema");

                    match input_reference_type {
                        TypeDefinition::Object(ot) => {
                            field.arguments = collection_arguments_for_named_type(&ot.name);
                        }
                        TypeDefinition::Interface(it) => {
                            field.arguments = collection_arguments_for_named_type(&it.name);
                        }
                        _ => unreachable!(
                            "referenced entity types can only be object or interface types"
                        ),
                    }
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::{prelude::DeploymentHash, schema::InputSchema};
    use graphql_parser::schema::*;
    use lazy_static::lazy_static;

    use super::ApiSchema;
    use crate::schema::ast;

    lazy_static! {
        static ref ID: DeploymentHash = DeploymentHash::new("apiTest").unwrap();
    }

    #[track_caller]
    fn parse(raw: &str) -> ApiSchema {
        let input_schema =
            InputSchema::parse(raw, ID.clone()).expect("Failed to parse input schema");
        input_schema
            .api_schema()
            .expect("Failed to derive API schema")
    }

    #[test]
    fn api_schema_contains_built_in_scalar_types() {
        let schema = parse("type User @entity { id: ID! }");

        schema
            .get_named_type("Boolean")
            .expect("Boolean type is missing in API schema");
        schema
            .get_named_type("ID")
            .expect("ID type is missing in API schema");
        schema
            .get_named_type("Int")
            .expect("Int type is missing in API schema");
        schema
            .get_named_type("BigDecimal")
            .expect("BigDecimal type is missing in API schema");
        schema
            .get_named_type("String")
            .expect("String type is missing in API schema");
        schema
            .get_named_type("Int8")
            .expect("Int8 type is missing in API schema");
    }

    #[test]
    fn api_schema_contains_order_direction_enum() {
        let schema = parse("type User @entity { id: ID!, name: String! }");

        let order_direction = schema
            .get_named_type("OrderDirection")
            .expect("OrderDirection type is missing in derived API schema");
        let enum_type = match order_direction {
            TypeDefinition::Enum(t) => Some(t),
            _ => None,
        }
        .expect("OrderDirection type is not an enum");

        let values: Vec<&str> = enum_type
            .values
            .iter()
            .map(|value| value.name.as_str())
            .collect();
        assert_eq!(values, ["asc", "desc"]);
    }

    #[test]
    fn api_schema_contains_query_type() {
        let schema = parse("type User @entity { id: ID! }");
        schema
            .get_named_type("Query")
            .expect("Root Query type is missing in API schema");
    }

    #[test]
    fn api_schema_contains_field_order_by_enum() {
        let schema = parse("type User @entity { id: ID!, name: String! }");

        let user_order_by = schema
            .get_named_type("User_orderBy")
            .expect("User_orderBy type is missing in derived API schema");

        let enum_type = match user_order_by {
            TypeDefinition::Enum(t) => Some(t),
            _ => None,
        }
        .expect("User_orderBy type is not an enum");

        let values: Vec<&str> = enum_type
            .values
            .iter()
            .map(|value| value.name.as_str())
            .collect();
        assert_eq!(values, ["id", "name"]);
    }

    #[test]
    fn api_schema_contains_field_order_by_enum_for_child_entity() {
        let schema = parse(
            r#"
              enum FurType {
                  NONE
                  FLUFFY
                  BRISTLY
              }

              type Pet @entity {
                  id: ID!
                  name: String!
                  mostHatedBy: [User!]!
                  mostLovedBy: [User!]!
              }

              interface Recipe {
                id: ID!
                name: String!
                author: User!
                lovedBy: [User!]!
                ingredients: [String!]!
              }

              type FoodRecipe implements Recipe @entity {
                id: ID!
                name: String!
                author: User!
                lovedBy: [User!]!
                ingredients: [String!]!
              }

              type DrinkRecipe implements Recipe @entity {
                id: ID!
                name: String!
                author: User!
                lovedBy: [User!]!
                ingredients: [String!]!
              }

              interface Meal {
                id: ID!
                name: String!
                mostHatedBy: [User!]!
                mostLovedBy: [User!]!
              }

              type Pizza implements Meal @entity {
                id: ID!
                name: String!
                toppings: [String!]!
                mostHatedBy: [User!]!
                mostLovedBy: [User!]!
              }

              type Burger implements Meal @entity {
                id: ID!
                name: String!
                bun: String!
                mostHatedBy: [User!]!
                mostLovedBy: [User!]!
              }

              type User @entity {
                  id: ID!
                  name: String!
                  favoritePetNames: [String!]
                  pets: [Pet!]!
                  favoriteFurType: FurType!
                  favoritePet: Pet!
                  leastFavoritePet: Pet @derivedFrom(field: "mostHatedBy")
                  mostFavoritePets: [Pet!] @derivedFrom(field: "mostLovedBy")
                  favoriteMeal: Meal!
                  leastFavoriteMeal: Meal @derivedFrom(field: "mostHatedBy")
                  mostFavoriteMeals: [Meal!] @derivedFrom(field: "mostLovedBy")
                  recipes: [Recipe!]! @derivedFrom(field: "author")
              }
            "#,
        );

        let user_order_by = schema
            .get_named_type("User_orderBy")
            .expect("User_orderBy type is missing in derived API schema");

        let enum_type = match user_order_by {
            TypeDefinition::Enum(t) => Some(t),
            _ => None,
        }
        .expect("User_orderBy type is not an enum");

        let values: Vec<&str> = enum_type
            .values
            .iter()
            .map(|value| value.name.as_str())
            .collect();

        assert_eq!(
            values,
            [
                "id",
                "name",
                "favoritePetNames",
                "pets",
                "favoriteFurType",
                "favoritePet",
                "favoritePet__id",
                "favoritePet__name",
                "leastFavoritePet",
                "leastFavoritePet__id",
                "leastFavoritePet__name",
                "mostFavoritePets",
                "favoriteMeal",
                "favoriteMeal__id",
                "favoriteMeal__name",
                "leastFavoriteMeal",
                "leastFavoriteMeal__id",
                "leastFavoriteMeal__name",
                "mostFavoriteMeals",
                "recipes",
            ]
        );

        let meal_order_by = schema
            .get_named_type("Meal_orderBy")
            .expect("Meal_orderBy type is missing in derived API schema");

        let enum_type = match meal_order_by {
            TypeDefinition::Enum(t) => Some(t),
            _ => None,
        }
        .expect("Meal_orderBy type is not an enum");

        let values: Vec<&str> = enum_type
            .values
            .iter()
            .map(|value| value.name.as_str())
            .collect();

        assert_eq!(values, ["id", "name", "mostHatedBy", "mostLovedBy",]);

        let recipe_order_by = schema
            .get_named_type("Recipe_orderBy")
            .expect("Recipe_orderBy type is missing in derived API schema");

        let enum_type = match recipe_order_by {
            TypeDefinition::Enum(t) => Some(t),
            _ => None,
        }
        .expect("Recipe_orderBy type is not an enum");

        let values: Vec<&str> = enum_type
            .values
            .iter()
            .map(|value| value.name.as_str())
            .collect();

        assert_eq!(
            values,
            [
                "id",
                "name",
                "author",
                "author__id",
                "author__name",
                "author__favoriteFurType",
                "lovedBy",
                "ingredients"
            ]
        );
    }

    #[test]
    fn api_schema_contains_object_type_filter_enum() {
        let schema = parse(
            r#"
              enum FurType {
                  NONE
                  FLUFFY
                  BRISTLY
              }

              type Pet @entity {
                  id: ID!
                  name: String!
                  mostHatedBy: [User!]!
                  mostLovedBy: [User!]!
              }

              type User @entity {
                  id: ID!
                  name: String!
                  favoritePetNames: [String!]
                  pets: [Pet!]!
                  favoriteFurType: FurType!
                  favoritePet: Pet!
                  leastFavoritePet: Pet @derivedFrom(field: "mostHatedBy")
                  mostFavoritePets: [Pet!] @derivedFrom(field: "mostLovedBy")
              }
            "#,
        );

        let user_filter = schema
            .get_named_type("User_filter")
            .expect("User_filter type is missing in derived API schema");

        let user_filter_type = match user_filter {
            TypeDefinition::InputObject(t) => Some(t),
            _ => None,
        }
        .expect("User_filter type is not an input object");

        assert_eq!(
            user_filter_type
                .fields
                .iter()
                .map(|field| field.name.clone())
                .collect::<Vec<String>>(),
            [
                "id",
                "id_not",
                "id_gt",
                "id_lt",
                "id_gte",
                "id_lte",
                "id_in",
                "id_not_in",
                "name",
                "name_not",
                "name_gt",
                "name_lt",
                "name_gte",
                "name_lte",
                "name_in",
                "name_not_in",
                "name_contains",
                "name_contains_nocase",
                "name_not_contains",
                "name_not_contains_nocase",
                "name_starts_with",
                "name_starts_with_nocase",
                "name_not_starts_with",
                "name_not_starts_with_nocase",
                "name_ends_with",
                "name_ends_with_nocase",
                "name_not_ends_with",
                "name_not_ends_with_nocase",
                "favoritePetNames",
                "favoritePetNames_not",
                "favoritePetNames_contains",
                "favoritePetNames_contains_nocase",
                "favoritePetNames_not_contains",
                "favoritePetNames_not_contains_nocase",
                "pets",
                "pets_not",
                "pets_contains",
                "pets_contains_nocase",
                "pets_not_contains",
                "pets_not_contains_nocase",
                "pets_",
                "favoriteFurType",
                "favoriteFurType_not",
                "favoriteFurType_in",
                "favoriteFurType_not_in",
                "favoritePet",
                "favoritePet_not",
                "favoritePet_gt",
                "favoritePet_lt",
                "favoritePet_gte",
                "favoritePet_lte",
                "favoritePet_in",
                "favoritePet_not_in",
                "favoritePet_contains",
                "favoritePet_contains_nocase",
                "favoritePet_not_contains",
                "favoritePet_not_contains_nocase",
                "favoritePet_starts_with",
                "favoritePet_starts_with_nocase",
                "favoritePet_not_starts_with",
                "favoritePet_not_starts_with_nocase",
                "favoritePet_ends_with",
                "favoritePet_ends_with_nocase",
                "favoritePet_not_ends_with",
                "favoritePet_not_ends_with_nocase",
                "favoritePet_",
                "leastFavoritePet_",
                "mostFavoritePets_",
                "_change_block",
                "and",
                "or"
            ]
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<String>>()
        );

        let pets_field = user_filter_type
            .fields
            .iter()
            .find(|field| field.name == "pets_")
            .expect("pets_ field is missing");

        assert_eq!(
            pets_field.value_type.to_string(),
            String::from("Pet_filter")
        );

        let pet_filter = schema
            .get_named_type("Pet_filter")
            .expect("Pet_filter type is missing in derived API schema");

        let pet_filter_type = match pet_filter {
            TypeDefinition::InputObject(t) => Some(t),
            _ => None,
        }
        .expect("Pet_filter type is not an input object");

        assert_eq!(
            pet_filter_type
                .fields
                .iter()
                .map(|field| field.name.clone())
                .collect::<Vec<String>>(),
            [
                "id",
                "id_not",
                "id_gt",
                "id_lt",
                "id_gte",
                "id_lte",
                "id_in",
                "id_not_in",
                "name",
                "name_not",
                "name_gt",
                "name_lt",
                "name_gte",
                "name_lte",
                "name_in",
                "name_not_in",
                "name_contains",
                "name_contains_nocase",
                "name_not_contains",
                "name_not_contains_nocase",
                "name_starts_with",
                "name_starts_with_nocase",
                "name_not_starts_with",
                "name_not_starts_with_nocase",
                "name_ends_with",
                "name_ends_with_nocase",
                "name_not_ends_with",
                "name_not_ends_with_nocase",
                "mostHatedBy",
                "mostHatedBy_not",
                "mostHatedBy_contains",
                "mostHatedBy_contains_nocase",
                "mostHatedBy_not_contains",
                "mostHatedBy_not_contains_nocase",
                "mostHatedBy_",
                "mostLovedBy",
                "mostLovedBy_not",
                "mostLovedBy_contains",
                "mostLovedBy_contains_nocase",
                "mostLovedBy_not_contains",
                "mostLovedBy_not_contains_nocase",
                "mostLovedBy_",
                "_change_block",
                "and",
                "or"
            ]
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<String>>()
        );

        let change_block_filter = user_filter_type
            .fields
            .iter()
            .find(move |p| match p.name.as_str() {
                "_change_block" => true,
                _ => false,
            })
            .expect("_change_block field is missing in User_filter");

        match &change_block_filter.value_type {
            Type::NamedType(name) => assert_eq!(name.as_str(), "BlockChangedFilter"),
            _ => panic!("_change_block field is not a named type"),
        }

        schema
            .get_named_type("BlockChangedFilter")
            .expect("BlockChangedFilter type is missing in derived API schema");
    }

    #[test]
    fn api_schema_contains_object_type_with_field_interface() {
        let schema = parse(
            r#"
              interface Pet {
                  id: ID!
                  name: String!
                  owner: User!
              }

              type Dog implements Pet @entity {
                id: ID!
                name: String!
                owner: User!
            }

              type Cat implements Pet @entity {
                id: ID!
                name: String!
                owner: User!
              }

              type User @entity {
                  id: ID!
                  name: String!
                  pets: [Pet!]! @derivedFrom(field: "owner")
                  favoritePet: Pet!
              }
            "#,
        );

        let user_filter = schema
            .get_named_type("User_filter")
            .expect("User_filter type is missing in derived API schema");

        let user_filter_type = match user_filter {
            TypeDefinition::InputObject(t) => Some(t),
            _ => None,
        }
        .expect("User_filter type is not an input object");

        assert_eq!(
            user_filter_type
                .fields
                .iter()
                .map(|field| field.name.clone())
                .collect::<Vec<String>>(),
            [
                "id",
                "id_not",
                "id_gt",
                "id_lt",
                "id_gte",
                "id_lte",
                "id_in",
                "id_not_in",
                "name",
                "name_not",
                "name_gt",
                "name_lt",
                "name_gte",
                "name_lte",
                "name_in",
                "name_not_in",
                "name_contains",
                "name_contains_nocase",
                "name_not_contains",
                "name_not_contains_nocase",
                "name_starts_with",
                "name_starts_with_nocase",
                "name_not_starts_with",
                "name_not_starts_with_nocase",
                "name_ends_with",
                "name_ends_with_nocase",
                "name_not_ends_with",
                "name_not_ends_with_nocase",
                "pets_",
                "favoritePet",
                "favoritePet_not",
                "favoritePet_gt",
                "favoritePet_lt",
                "favoritePet_gte",
                "favoritePet_lte",
                "favoritePet_in",
                "favoritePet_not_in",
                "favoritePet_contains",
                "favoritePet_contains_nocase",
                "favoritePet_not_contains",
                "favoritePet_not_contains_nocase",
                "favoritePet_starts_with",
                "favoritePet_starts_with_nocase",
                "favoritePet_not_starts_with",
                "favoritePet_not_starts_with_nocase",
                "favoritePet_ends_with",
                "favoritePet_ends_with_nocase",
                "favoritePet_not_ends_with",
                "favoritePet_not_ends_with_nocase",
                "favoritePet_",
                "_change_block",
                "and",
                "or"
            ]
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<String>>()
        );

        let change_block_filter = user_filter_type
            .fields
            .iter()
            .find(move |p| match p.name.as_str() {
                "_change_block" => true,
                _ => false,
            })
            .expect("_change_block field is missing in User_filter");

        match &change_block_filter.value_type {
            Type::NamedType(name) => assert_eq!(name.as_str(), "BlockChangedFilter"),
            _ => panic!("_change_block field is not a named type"),
        }

        schema
            .get_named_type("BlockChangedFilter")
            .expect("BlockChangedFilter type is missing in derived API schema");
    }

    #[test]
    fn api_schema_contains_object_fields_on_query_type() {
        let schema = parse(
            "type User @entity { id: ID!, name: String! } type UserProfile @entity { id: ID!, title: String! }",
        );

        let query_type = schema
            .get_named_type("Query")
            .expect("Query type is missing in derived API schema");

        let user_singular_field = match query_type {
            TypeDefinition::Object(t) => ast::get_field(t, "user"),
            _ => None,
        }
        .expect("\"user\" field is missing on Query type");

        assert_eq!(
            user_singular_field.field_type,
            Type::NamedType("User".to_string())
        );

        assert_eq!(
            user_singular_field
                .arguments
                .iter()
                .map(|input_value| input_value.name.clone())
                .collect::<Vec<String>>(),
            vec![
                "id".to_string(),
                "block".to_string(),
                "subgraphError".to_string()
            ],
        );

        let user_plural_field = match query_type {
            TypeDefinition::Object(t) => ast::get_field(t, "users"),
            _ => None,
        }
        .expect("\"users\" field is missing on Query type");

        assert_eq!(
            user_plural_field.field_type,
            Type::NonNullType(Box::new(Type::ListType(Box::new(Type::NonNullType(
                Box::new(Type::NamedType("User".to_string()))
            )))))
        );

        assert_eq!(
            user_plural_field
                .arguments
                .iter()
                .map(|input_value| input_value.name.clone())
                .collect::<Vec<String>>(),
            [
                "skip",
                "first",
                "orderBy",
                "orderDirection",
                "where",
                "block",
                "subgraphError",
            ]
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<String>>()
        );

        let user_profile_singular_field = match query_type {
            TypeDefinition::Object(t) => ast::get_field(t, "userProfile"),
            _ => None,
        }
        .expect("\"userProfile\" field is missing on Query type");

        assert_eq!(
            user_profile_singular_field.field_type,
            Type::NamedType("UserProfile".to_string())
        );

        let user_profile_plural_field = match query_type {
            TypeDefinition::Object(t) => ast::get_field(t, "userProfiles"),
            _ => None,
        }
        .expect("\"userProfiles\" field is missing on Query type");

        assert_eq!(
            user_profile_plural_field.field_type,
            Type::NonNullType(Box::new(Type::ListType(Box::new(Type::NonNullType(
                Box::new(Type::NamedType("UserProfile".to_string()))
            )))))
        );
    }

    #[test]
    fn api_schema_contains_interface_fields_on_query_type() {
        let schema = parse(
            "
            interface Node { id: ID!, name: String! }
            type User implements Node @entity { id: ID!, name: String!, email: String }
            ",
        );

        let query_type = schema
            .get_named_type("Query")
            .expect("Query type is missing in derived API schema");

        let singular_field = match query_type {
            TypeDefinition::Object(ref t) => ast::get_field(t, "node"),
            _ => None,
        }
        .expect("\"node\" field is missing on Query type");

        assert_eq!(
            singular_field.field_type,
            Type::NamedType("Node".to_string())
        );

        assert_eq!(
            singular_field
                .arguments
                .iter()
                .map(|input_value| input_value.name.clone())
                .collect::<Vec<String>>(),
            vec![
                "id".to_string(),
                "block".to_string(),
                "subgraphError".to_string()
            ],
        );

        let plural_field = match query_type {
            TypeDefinition::Object(ref t) => ast::get_field(t, "nodes"),
            _ => None,
        }
        .expect("\"nodes\" field is missing on Query type");

        assert_eq!(
            plural_field.field_type,
            Type::NonNullType(Box::new(Type::ListType(Box::new(Type::NonNullType(
                Box::new(Type::NamedType("Node".to_string()))
            )))))
        );

        assert_eq!(
            plural_field
                .arguments
                .iter()
                .map(|input_value| input_value.name.clone())
                .collect::<Vec<String>>(),
            [
                "skip",
                "first",
                "orderBy",
                "orderDirection",
                "where",
                "block",
                "subgraphError"
            ]
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<String>>()
        );
    }

    #[test]
    fn api_schema_contains_fulltext_query_field_on_query_type() {
        const SCHEMA: &str = r#"
type _Schema_ @fulltext(
  name: "metadata"
  language: en
  algorithm: rank
  include: [
    {
      entity: "Gravatar",
      fields: [
        { name: "displayName"},
        { name: "imageUrl"},
      ]
    }
  ]
)
type Gravatar @entity {
  id: ID!
  owner: Bytes!
  displayName: String!
  imageUrl: String!
}
"#;
        let schema = parse(SCHEMA);

        let query_type = schema
            .get_named_type("Query")
            .expect("Query type is missing in derived API schema");

        let _metadata_field = match query_type {
            TypeDefinition::Object(t) => ast::get_field(t, &String::from("metadata")),
            _ => None,
        }
        .expect("\"metadata\" field is missing on Query type");
    }

    #[test]
    fn intf_implements_intf() {
        const SCHEMA: &str = r#"
          interface Legged {
            legs: Int!
          }

          interface Animal implements Legged {
            id: Bytes!
            legs: Int!
          }

          type Zoo @entity {
            id: Bytes!
            animals: [Animal!]
          }
          "#;
        // This used to fail in API schema construction; we just want to
        // make sure that generating an API schema works. The issue was that
        // `Zoo.animals` has an interface type, and that interface
        // implements another interface which we tried to look up as an
        // object type
        let _schema = parse(SCHEMA);
    }
}
