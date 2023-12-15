use graph::components::store::QueryPermit;
use graph::data::graphql::ext::{FieldExt, TypeDefinitionExt};
use graph::data::query::Trace;
use graphql_parser::Pos;
use std::collections::BTreeMap;

use graph::data::graphql::{object, DocumentExt, ObjectOrInterface};
use graph::prelude::*;

use crate::execution::ast as a;
use crate::prelude::*;
use graph::schema::{ast as sast, Schema};

type TypeObjectsMap = BTreeMap<String, r::Value>;

/// Our Schema has the introspection schema mixed in. When we build the
/// `TypeObjectsMap`, suppress types and fields that belong to the
/// introspection schema
fn schema_type_objects(schema: &Schema) -> TypeObjectsMap {
    sast::get_type_definitions(&schema.document)
        .iter()
        .filter(|def| !def.is_introspection())
        .fold(BTreeMap::new(), |mut type_objects, typedef| {
            let type_name = sast::get_type_name(typedef);
            if !type_objects.contains_key(type_name) {
                let type_object = type_definition_object(schema, &mut type_objects, typedef);
                type_objects.insert(type_name.to_owned(), type_object);
            }
            type_objects
        })
}

fn type_object(schema: &Schema, type_objects: &mut TypeObjectsMap, t: &s::Type) -> r::Value {
    match t {
        // We store the name of the named type here to be able to resolve it dynamically later
        s::Type::NamedType(s) => r::Value::String(s.clone()),
        s::Type::ListType(ref inner) => list_type_object(schema, type_objects, inner),
        s::Type::NonNullType(ref inner) => non_null_type_object(schema, type_objects, inner),
    }
}

fn list_type_object(
    schema: &Schema,
    type_objects: &mut TypeObjectsMap,
    inner_type: &s::Type,
) -> r::Value {
    object! {
        kind: r::Value::Enum(String::from("LIST")),
        ofType: type_object(schema, type_objects, inner_type),
    }
}

fn non_null_type_object(
    schema: &Schema,
    type_objects: &mut TypeObjectsMap,
    inner_type: &s::Type,
) -> r::Value {
    object! {
        kind: r::Value::Enum(String::from("NON_NULL")),
        ofType: type_object(schema, type_objects, inner_type),
    }
}

fn type_definition_object(
    schema: &Schema,
    type_objects: &mut TypeObjectsMap,
    typedef: &s::TypeDefinition,
) -> r::Value {
    let type_name = sast::get_type_name(typedef);

    type_objects.get(type_name).cloned().unwrap_or_else(|| {
        let type_object = match typedef {
            s::TypeDefinition::Enum(enum_type) => enum_type_object(enum_type),
            s::TypeDefinition::InputObject(input_object_type) => {
                input_object_type_object(schema, type_objects, input_object_type)
            }
            s::TypeDefinition::Interface(interface_type) => {
                interface_type_object(schema, type_objects, interface_type)
            }
            s::TypeDefinition::Object(object_type) => {
                object_type_object(schema, type_objects, object_type)
            }
            s::TypeDefinition::Scalar(scalar_type) => scalar_type_object(scalar_type),
            s::TypeDefinition::Union(union_type) => union_type_object(schema, union_type),
        };

        type_objects.insert(type_name.to_owned(), type_object.clone());
        type_object
    })
}

fn enum_type_object(enum_type: &s::EnumType) -> r::Value {
    object! {
        kind: r::Value::Enum(String::from("ENUM")),
        name: enum_type.name.clone(),
        description: enum_type.description.clone(),
        enumValues: enum_values(enum_type),
    }
}

fn enum_values(enum_type: &s::EnumType) -> r::Value {
    r::Value::List(enum_type.values.iter().map(enum_value).collect())
}

fn enum_value(enum_value: &s::EnumValue) -> r::Value {
    object! {
        name: enum_value.name.clone(),
        description: enum_value.description.clone(),
        isDeprecated: false,
        deprecationReason: r::Value::Null,
    }
}

fn input_object_type_object(
    schema: &Schema,
    type_objects: &mut TypeObjectsMap,
    input_object_type: &s::InputObjectType,
) -> r::Value {
    let input_values = input_values(schema, type_objects, &input_object_type.fields);
    object! {
        name: input_object_type.name.clone(),
        kind: r::Value::Enum(String::from("INPUT_OBJECT")),
        description: input_object_type.description.clone(),
        inputFields: input_values,
    }
}

fn interface_type_object(
    schema: &Schema,
    type_objects: &mut TypeObjectsMap,
    interface_type: &s::InterfaceType,
) -> r::Value {
    object! {
        name: interface_type.name.clone(),
        kind: r::Value::Enum(String::from("INTERFACE")),
        description: interface_type.description.clone(),
        fields:
            field_objects(schema, type_objects, &interface_type.fields),
        possibleTypes: schema.types_for_interface()[interface_type.name.as_str()]
            .iter()
            .map(|object_type| r::Value::String(object_type.name.clone()))
            .collect::<Vec<_>>(),
    }
}

fn object_type_object(
    schema: &Schema,
    type_objects: &mut TypeObjectsMap,
    object_type: &s::ObjectType,
) -> r::Value {
    type_objects
        .get(&object_type.name)
        .cloned()
        .unwrap_or_else(|| {
            let type_object = object! {
                kind: r::Value::Enum(String::from("OBJECT")),
                name: object_type.name.clone(),
                description: object_type.description.clone(),
                fields: field_objects(schema, type_objects, &object_type.fields),
                interfaces: object_interfaces(schema, type_objects, object_type),
            };

            type_objects.insert(object_type.name.clone(), type_object.clone());
            type_object
        })
}

fn field_objects(
    schema: &Schema,
    type_objects: &mut TypeObjectsMap,
    fields: &[s::Field],
) -> r::Value {
    r::Value::List(
        fields
            .iter()
            .filter(|field| !field.is_introspection())
            .map(|field| field_object(schema, type_objects, field))
            .collect(),
    )
}

fn field_object(schema: &Schema, type_objects: &mut TypeObjectsMap, field: &s::Field) -> r::Value {
    object! {
        name: field.name.clone(),
        description: field.description.clone(),
        args: input_values(schema, type_objects, &field.arguments),
        type: type_object(schema, type_objects, &field.field_type),
        isDeprecated: false,
        deprecationReason: r::Value::Null,
    }
}

fn object_interfaces(
    schema: &Schema,
    type_objects: &mut TypeObjectsMap,
    object_type: &s::ObjectType,
) -> r::Value {
    r::Value::List(
        schema
            .interfaces_for_type(&object_type.name)
            .unwrap_or(&vec![])
            .iter()
            .map(|typedef| interface_type_object(schema, type_objects, typedef))
            .collect(),
    )
}

fn scalar_type_object(scalar_type: &s::ScalarType) -> r::Value {
    object! {
        name: scalar_type.name.clone(),
        kind: r::Value::Enum(String::from("SCALAR")),
        description: scalar_type.description.clone(),
        isDeprecated: false,
        deprecationReason: r::Value::Null,
    }
}

fn union_type_object(schema: &Schema, union_type: &s::UnionType) -> r::Value {
    object! {
        name: union_type.name.clone(),
        kind: r::Value::Enum(String::from("UNION")),
        description: union_type.description.clone(),
        possibleTypes:
            schema.document.get_object_type_definitions()
                .iter()
                .filter(|object_type| {
                    object_type
                        .implements_interfaces
                        .iter()
                        .any(|implemented_name| implemented_name == &union_type.name)
                })
                .map(|object_type| r::Value::String(object_type.name.clone()))
                .collect::<Vec<_>>(),
    }
}

fn schema_directive_objects(schema: &Schema, type_objects: &mut TypeObjectsMap) -> r::Value {
    r::Value::List(
        schema
            .document
            .definitions
            .iter()
            .filter_map(|d| match d {
                s::Definition::DirectiveDefinition(dd) => Some(dd),
                _ => None,
            })
            .map(|dd| directive_object(schema, type_objects, dd))
            .collect(),
    )
}

fn directive_object(
    schema: &Schema,
    type_objects: &mut TypeObjectsMap,
    directive: &s::DirectiveDefinition,
) -> r::Value {
    object! {
        name: directive.name.clone(),
        description: directive.description.clone(),
        locations: directive_locations(directive),
        args: input_values(schema, type_objects, &directive.arguments),
    }
}

fn directive_locations(directive: &s::DirectiveDefinition) -> r::Value {
    r::Value::List(
        directive
            .locations
            .iter()
            .map(|location| location.as_str())
            .map(|name| r::Value::Enum(name.to_owned()))
            .collect(),
    )
}

fn input_values(
    schema: &Schema,
    type_objects: &mut TypeObjectsMap,
    input_values: &[s::InputValue],
) -> Vec<r::Value> {
    input_values
        .iter()
        .map(|value| input_value(schema, type_objects, value))
        .collect()
}

fn input_value(
    schema: &Schema,
    type_objects: &mut TypeObjectsMap,
    input_value: &s::InputValue,
) -> r::Value {
    object! {
        name: input_value.name.clone(),
        description: input_value.description.clone(),
        type: type_object(schema, type_objects, &input_value.value_type),
        defaultValue:
            input_value
                .default_value
                .as_ref()
                .map_or(r::Value::Null, |value| {
                    r::Value::String(format!("{}", value))
                }),
    }
}

#[derive(Clone)]
pub struct IntrospectionResolver {
    _logger: Logger,
    type_objects: TypeObjectsMap,
    directives: r::Value,
}

impl IntrospectionResolver {
    pub fn new(logger: &Logger, schema: &Schema) -> Self {
        let logger = logger.new(o!("component" => "IntrospectionResolver"));

        // Generate queryable objects for all types in the schema
        let mut type_objects = schema_type_objects(schema);

        // Generate queryable objects for all directives in the schema
        let directives = schema_directive_objects(schema, &mut type_objects);

        IntrospectionResolver {
            _logger: logger,
            type_objects,
            directives,
        }
    }

    fn schema_object(&self) -> r::Value {
        object! {
            queryType:
                self.type_objects
                    .get(&String::from("Query"))
                    .cloned(),
            subscriptionType:
                self.type_objects
                    .get(&String::from("Subscription"))
                    .cloned(),
            mutationType: r::Value::Null,
            types: self.type_objects.values().cloned().collect::<Vec<_>>(),
            directives: self.directives.clone(),
        }
    }

    fn type_object(&self, name: &r::Value) -> r::Value {
        match name {
            r::Value::String(s) => Some(s),
            _ => None,
        }
        .and_then(|name| self.type_objects.get(name).cloned())
        .unwrap_or(r::Value::Null)
    }
}

/// A GraphQL resolver that can resolve entities, enum values, scalar types and interfaces/unions.
#[async_trait]
impl Resolver for IntrospectionResolver {
    // `IntrospectionResolver` is not used as a "top level" resolver,
    // see `fn as_introspection_context`, so this value is irrelevant.
    const CACHEABLE: bool = false;

    async fn query_permit(&self) -> Result<QueryPermit, QueryExecutionError> {
        unreachable!()
    }

    fn prefetch(
        &self,
        _: &ExecutionContext<Self>,
        _: &a::SelectionSet,
    ) -> Result<(Option<r::Value>, Trace), Vec<QueryExecutionError>> {
        Ok((None, Trace::None))
    }

    async fn resolve_objects(
        &self,
        prefetched_objects: Option<r::Value>,
        field: &a::Field,
        _field_definition: &s::Field,
        _object_type: ObjectOrInterface<'_>,
    ) -> Result<r::Value, QueryExecutionError> {
        match field.name.as_str() {
            "possibleTypes" => {
                let type_names = match prefetched_objects {
                    Some(r::Value::List(type_names)) => Some(type_names),
                    _ => None,
                }
                .unwrap_or_default();

                if !type_names.is_empty() {
                    Ok(r::Value::List(
                        type_names
                            .iter()
                            .filter_map(|type_name| match type_name {
                                r::Value::String(ref type_name) => Some(type_name),
                                _ => None,
                            })
                            .filter_map(|type_name| self.type_objects.get(type_name).cloned())
                            .map(r::Value::try_from)
                            .collect::<Result<_, _>>()
                            .map_err(|v| {
                                QueryExecutionError::ValueParseError(
                                    "internal error resolving type name".to_string(),
                                    v.to_string(),
                                )
                            })?,
                    ))
                } else {
                    Ok(r::Value::Null)
                }
            }
            _ => Ok(prefetched_objects.unwrap_or(r::Value::Null)),
        }
    }

    async fn resolve_object(
        &self,
        prefetched_object: Option<r::Value>,
        field: &a::Field,
        _field_definition: &s::Field,
        _object_type: ObjectOrInterface<'_>,
    ) -> Result<r::Value, QueryExecutionError> {
        let object = match field.name.as_str() {
            "__schema" => self.schema_object(),
            "__type" => {
                let name = field.argument_value("name").ok_or_else(|| {
                    QueryExecutionError::MissingArgumentError(
                        Pos::default(),
                        "missing argument `name` in `__type(name: String!)`".to_owned(),
                    )
                })?;
                self.type_object(name)
            }
            "type" | "ofType" => match prefetched_object {
                Some(r::Value::String(type_name)) => self
                    .type_objects
                    .get(&type_name)
                    .cloned()
                    .unwrap_or(r::Value::Null),
                Some(v) => v,
                None => r::Value::Null,
            },
            _ => prefetched_object.unwrap_or(r::Value::Null),
        };
        Ok(object)
    }
}
