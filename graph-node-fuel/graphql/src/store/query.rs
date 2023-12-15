use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};
use std::mem::discriminant;

use graph::data::graphql::ext::DirectiveFinder;
use graph::data::graphql::ObjectOrInterface;
use graph::data::graphql::TypeExt as _;
use graph::data::value::Object;
use graph::data::value::Value as DataValue;
use graph::prelude::*;
use graph::schema::ast::{self as sast, FilterOp};
use graph::schema::{ApiSchema, EntityType, InputSchema};

use crate::execution::ast as a;

use super::prefetch::SelectedAttributes;

#[derive(Debug)]
enum OrderDirection {
    Ascending,
    Descending,
}

pub(crate) struct SchemaPair {
    pub api: Arc<ApiSchema>,
    pub input: InputSchema,
}

/// Builds a EntityQuery from GraphQL arguments.
///
/// Panics if `entity` is not present in `schema`.
pub(crate) fn build_query<'a>(
    entity: impl Into<ObjectOrInterface<'a>>,
    block: BlockNumber,
    field: &a::Field,
    types_for_interface: &'a BTreeMap<String, Vec<s::ObjectType>>,
    max_first: u32,
    max_skip: u32,
    mut column_names: SelectedAttributes,
    schema: &SchemaPair,
) -> Result<EntityQuery, QueryExecutionError> {
    let entity = entity.into();
    let entity_types = EntityCollection::All(match &entity {
        ObjectOrInterface::Object(object) => {
            let selected_columns = column_names.get(object);
            let entity_type = schema.input.entity_type(*object).unwrap();
            vec![(entity_type, selected_columns)]
        }
        ObjectOrInterface::Interface(interface) => types_for_interface[&interface.name]
            .iter()
            .map(|o| {
                let selected_columns = column_names.get(o);
                let entity_type = schema.input.entity_type(o).unwrap();
                (entity_type, selected_columns)
            })
            .collect(),
    });
    let mut query = EntityQuery::new(parse_subgraph_id(entity)?, block, entity_types)
        .range(build_range(field, max_first, max_skip)?);
    if let Some(filter) = build_filter(entity, field, schema)? {
        query = query.filter(filter);
    }
    let order = match (
        build_order_by(entity, field, schema)?,
        build_order_direction(field)?,
    ) {
        (Some((attr, value_type, None)), OrderDirection::Ascending) => {
            EntityOrder::Ascending(attr, value_type)
        }
        (Some((attr, value_type, None)), OrderDirection::Descending) => {
            EntityOrder::Descending(attr, value_type)
        }
        (Some((attr, _, Some(child))), OrderDirection::Ascending) => {
            if ENV_VARS.graphql.disable_child_sorting {
                return Err(QueryExecutionError::NotSupported(
                    "Sorting by child attributes is not supported".to_string(),
                ));
            }
            match child {
                OrderByChild::Object(child) => {
                    EntityOrder::ChildAscending(EntityOrderByChild::Object(
                        EntityOrderByChildInfo {
                            sort_by_attribute: attr,
                            join_attribute: child.join_attribute,
                            derived: child.derived,
                        },
                        child.entity_type,
                    ))
                }
                OrderByChild::Interface(child) => {
                    EntityOrder::ChildAscending(EntityOrderByChild::Interface(
                        EntityOrderByChildInfo {
                            sort_by_attribute: attr,
                            join_attribute: child.join_attribute,
                            derived: child.derived,
                        },
                        child.entity_types,
                    ))
                }
            }
        }
        (Some((attr, _, Some(child))), OrderDirection::Descending) => {
            if ENV_VARS.graphql.disable_child_sorting {
                return Err(QueryExecutionError::NotSupported(
                    "Sorting by child attributes is not supported".to_string(),
                ));
            }
            match child {
                OrderByChild::Object(child) => {
                    EntityOrder::ChildDescending(EntityOrderByChild::Object(
                        EntityOrderByChildInfo {
                            sort_by_attribute: attr,
                            join_attribute: child.join_attribute,
                            derived: child.derived,
                        },
                        child.entity_type,
                    ))
                }
                OrderByChild::Interface(child) => {
                    EntityOrder::ChildDescending(EntityOrderByChild::Interface(
                        EntityOrderByChildInfo {
                            sort_by_attribute: attr,
                            join_attribute: child.join_attribute,
                            derived: child.derived,
                        },
                        child.entity_types,
                    ))
                }
            }
        }
        (None, _) => EntityOrder::Default,
    };
    query = query.order(order);
    Ok(query)
}

/// Parses GraphQL arguments into a EntityRange, if present.
fn build_range(
    field: &a::Field,
    max_first: u32,
    max_skip: u32,
) -> Result<EntityRange, QueryExecutionError> {
    let first = match field.argument_value("first") {
        Some(r::Value::Int(n)) => {
            let n = *n;
            if n > 0 && n <= (max_first as i64) {
                n as u32
            } else {
                return Err(QueryExecutionError::RangeArgumentsError(
                    "first", max_first, n,
                ));
            }
        }
        Some(r::Value::Null) | None => 100,
        _ => unreachable!("first is an Int with a default value"),
    };

    let skip = match field.argument_value("skip") {
        Some(r::Value::Int(n)) => {
            let n = *n;
            if n >= 0 && n <= (max_skip as i64) {
                n as u32
            } else {
                return Err(QueryExecutionError::RangeArgumentsError(
                    "skip", max_skip, n,
                ));
            }
        }
        Some(r::Value::Null) | None => 0,
        _ => unreachable!("skip is an Int with a default value"),
    };

    Ok(EntityRange {
        first: Some(first),
        skip,
    })
}

/// Parses GraphQL arguments into an EntityFilter, if present.
fn build_filter(
    entity: ObjectOrInterface,
    field: &a::Field,
    schema: &SchemaPair,
) -> Result<Option<EntityFilter>, QueryExecutionError> {
    let where_filter = match field.argument_value("where") {
        Some(r::Value::Object(object)) => match build_filter_from_object(entity, object, schema) {
            Ok(filter) => Ok(Some(EntityFilter::And(filter))),
            Err(e) => Err(e),
        },
        Some(r::Value::Null) | None => Ok(None),
        _ => Err(QueryExecutionError::InvalidFilterError),
    }?;

    let text_filter = match field.argument_value("text") {
        Some(r::Value::Object(filter)) => build_fulltext_filter_from_object(filter),
        None => Ok(None),
        _ => Err(QueryExecutionError::InvalidFilterError),
    }?;

    match (where_filter, text_filter) {
        (None, None) => Ok(None),
        (Some(f), None) | (None, Some(f)) => Ok(Some(f)),
        (Some(w), Some(t)) => Ok(Some(EntityFilter::And(vec![t, w]))),
    }
}

fn build_fulltext_filter_from_object(
    object: &Object,
) -> Result<Option<EntityFilter>, QueryExecutionError> {
    object.iter().next().map_or(
        Err(QueryExecutionError::FulltextQueryRequiresFilter),
        |(key, value)| {
            if let r::Value::String(s) = value {
                Ok(Some(EntityFilter::Fulltext(
                    key.to_string(),
                    Value::String(s.clone()),
                )))
            } else {
                Err(QueryExecutionError::FulltextQueryRequiresFilter)
            }
        },
    )
}

fn parse_change_block_filter(value: &r::Value) -> Result<BlockNumber, QueryExecutionError> {
    match value {
        r::Value::Object(object) => i32::try_from_value(
            object
                .get("number_gte")
                .ok_or(QueryExecutionError::InvalidFilterError)?,
        )
        .map_err(|_| QueryExecutionError::InvalidFilterError),
        _ => Err(QueryExecutionError::InvalidFilterError),
    }
}

/// Parses a GraphQL Filter Value into an EntityFilter.
fn build_entity_filter(
    field_name: String,
    operation: FilterOp,
    store_value: Value,
) -> Result<EntityFilter, QueryExecutionError> {
    match operation {
        FilterOp::Not => Ok(EntityFilter::Not(field_name, store_value)),
        FilterOp::GreaterThan => Ok(EntityFilter::GreaterThan(field_name, store_value)),
        FilterOp::LessThan => Ok(EntityFilter::LessThan(field_name, store_value)),
        FilterOp::GreaterOrEqual => Ok(EntityFilter::GreaterOrEqual(field_name, store_value)),
        FilterOp::LessOrEqual => Ok(EntityFilter::LessOrEqual(field_name, store_value)),
        FilterOp::In => Ok(EntityFilter::In(
            field_name,
            list_values(store_value, "_in")?,
        )),
        FilterOp::NotIn => Ok(EntityFilter::NotIn(
            field_name,
            list_values(store_value, "_not_in")?,
        )),
        FilterOp::Contains => Ok(EntityFilter::Contains(field_name, store_value)),
        FilterOp::ContainsNoCase => Ok(EntityFilter::ContainsNoCase(field_name, store_value)),
        FilterOp::NotContains => Ok(EntityFilter::NotContains(field_name, store_value)),
        FilterOp::NotContainsNoCase => Ok(EntityFilter::NotContainsNoCase(field_name, store_value)),
        FilterOp::StartsWith => Ok(EntityFilter::StartsWith(field_name, store_value)),
        FilterOp::StartsWithNoCase => Ok(EntityFilter::StartsWithNoCase(field_name, store_value)),
        FilterOp::NotStartsWith => Ok(EntityFilter::NotStartsWith(field_name, store_value)),
        FilterOp::NotStartsWithNoCase => {
            Ok(EntityFilter::NotStartsWithNoCase(field_name, store_value))
        }
        FilterOp::EndsWith => Ok(EntityFilter::EndsWith(field_name, store_value)),
        FilterOp::EndsWithNoCase => Ok(EntityFilter::EndsWithNoCase(field_name, store_value)),
        FilterOp::NotEndsWith => Ok(EntityFilter::NotEndsWith(field_name, store_value)),
        FilterOp::NotEndsWithNoCase => Ok(EntityFilter::NotEndsWithNoCase(field_name, store_value)),
        FilterOp::Equal => Ok(EntityFilter::Equal(field_name, store_value)),
        _ => unreachable!(),
    }
}

/// Iterate over the list and generate an EntityFilter from it
fn build_list_filter_from_value(
    entity: ObjectOrInterface,
    schema: &SchemaPair,
    value: &r::Value,
) -> Result<Vec<EntityFilter>, QueryExecutionError> {
    // We have object like this
    // { or: [{ name: \"John\", id: \"m1\" }, { mainBand: \"b2\" }] }
    match value {
        r::Value::List(list) => Ok(list
            .iter()
            .map(|item| {
                // It is each filter in the object
                // { name: \"John\", id: \"m1\" }
                // the fields within the object are ANDed together
                match item {
                    r::Value::Object(object) => Ok(EntityFilter::And(build_filter_from_object(
                        entity, object, schema,
                    )?)),
                    _ => Err(QueryExecutionError::InvalidFilterError),
                }
            })
            .collect::<Result<Vec<EntityFilter>, QueryExecutionError>>()?),
        _ => Err(QueryExecutionError::InvalidFilterError),
    }
}

/// build a filter which has list of nested filters
fn build_list_filter_from_object<'a>(
    entity: ObjectOrInterface,
    object: &Object,
    schema: &SchemaPair,
) -> Result<Vec<EntityFilter>, QueryExecutionError> {
    Ok(object
        .iter()
        .map(|(_, value)| build_list_filter_from_value(entity, schema, value))
        .collect::<Result<Vec<Vec<EntityFilter>>, QueryExecutionError>>()?
        .into_iter()
        // We iterate an object so all entity filters are flattened into one list
        .flatten()
        .collect::<Vec<EntityFilter>>())
}

/// Parses a GraphQL input object into an EntityFilter, if present.
fn build_filter_from_object<'a>(
    entity: ObjectOrInterface,
    object: &Object,
    schema: &SchemaPair,
) -> Result<Vec<EntityFilter>, QueryExecutionError> {
    object
        .iter()
        .map(|(key, value)| {
            // Special handling for _change_block input filter since its not a
            // standard entity filter that is based on entity structure/fields
            if key == "_change_block" {
                return match parse_change_block_filter(value) {
                    Ok(block_number) => Ok(EntityFilter::ChangeBlockGte(block_number)),
                    Err(e) => Err(e),
                };
            }
            use self::sast::FilterOp::*;
            let (field_name, op) = sast::parse_field_as_filter(key);

            Ok(match op {
                And => {
                    if ENV_VARS.graphql.disable_bool_filters {
                        return Err(QueryExecutionError::NotSupported(
                            "Boolean filters are not supported".to_string(),
                        ));
                    }

                    return Ok(EntityFilter::And(build_list_filter_from_object(
                        entity, object, schema,
                    )?));
                }
                Or => {
                    if ENV_VARS.graphql.disable_bool_filters {
                        return Err(QueryExecutionError::NotSupported(
                            "Boolean filters are not supported".to_string(),
                        ));
                    }

                    return Ok(EntityFilter::Or(build_list_filter_from_object(
                        entity, object, schema,
                    )?));
                }
                Child => match value {
                    DataValue::Object(obj) => {
                        build_child_filter_from_object(entity, field_name, obj, schema)?
                    }
                    _ => {
                        let field = sast::get_field(entity, &field_name).ok_or_else(|| {
                            QueryExecutionError::EntityFieldError(
                                entity.name().to_owned(),
                                field_name.clone(),
                            )
                        })?;
                        let ty = &field.field_type;
                        return Err(QueryExecutionError::AttributeTypeError(
                            value.to_string(),
                            ty.to_string(),
                        ));
                    }
                },
                _ => {
                    let field = sast::get_field(entity, &field_name).ok_or_else(|| {
                        QueryExecutionError::EntityFieldError(
                            entity.name().to_owned(),
                            field_name.clone(),
                        )
                    })?;
                    let ty = &field.field_type;
                    let store_value = Value::from_query_value(value, ty)?;
                    return build_entity_filter(field_name, op, store_value);
                }
            })
        })
        .collect::<Result<Vec<EntityFilter>, QueryExecutionError>>()
}

fn build_child_filter_from_object(
    entity: ObjectOrInterface,
    field_name: String,
    object: &Object,
    schema: &SchemaPair,
) -> Result<EntityFilter, QueryExecutionError> {
    let field = entity
        .field(&field_name)
        .ok_or(QueryExecutionError::InvalidFilterError)?;
    let type_name = &field.field_type.get_base_type();
    let child_entity = schema
        .api
        .object_or_interface(type_name)
        .ok_or(QueryExecutionError::InvalidFilterError)?;
    let child_entity_type = schema.input.entity_type(child_entity)?;
    let filter = Box::new(EntityFilter::And(build_filter_from_object(
        child_entity,
        object,
        schema,
    )?));
    let derived = field.is_derived();
    let attr = match derived {
        true => sast::get_derived_from_field(child_entity, field)
            .ok_or(QueryExecutionError::InvalidFilterError)?
            .name
            .to_string(),
        false => field_name.clone(),
    };

    if child_entity.is_interface() {
        Ok(EntityFilter::Or(
            child_entity
                .object_types(schema.api.schema())
                .ok_or(QueryExecutionError::AbstractTypeError(
                    "Interface is not implemented by any types".to_string(),
                ))?
                .iter()
                .map(|object_type| {
                    schema.input.entity_type(*object_type).map(|entity_type| {
                        EntityFilter::Child(Child {
                            attr: attr.clone(),
                            entity_type,
                            filter: filter.clone(),
                            derived,
                        })
                    })
                })
                .collect::<Result<_, _>>()?,
        ))
    } else if entity.is_interface() {
        Ok(EntityFilter::Or(
            entity
                .object_types(schema.api.schema())
                .ok_or(QueryExecutionError::AbstractTypeError(
                    "Interface is not implemented by any types".to_string(),
                ))?
                .iter()
                .map(|object_type| {
                    let field = object_type
                        .fields
                        .iter()
                        .find(|f| f.name == field_name.clone())
                        .ok_or(QueryExecutionError::InvalidFilterError)?;
                    let derived = field.is_derived();

                    let attr = match derived {
                        true => sast::get_derived_from_field(child_entity, field)
                            .ok_or(QueryExecutionError::InvalidFilterError)?
                            .name
                            .to_string(),
                        false => field_name.clone(),
                    };

                    Ok(EntityFilter::Child(Child {
                        attr,
                        entity_type: child_entity_type.clone(),
                        filter: filter.clone(),
                        derived,
                    }))
                })
                .collect::<Result<Vec<EntityFilter>, QueryExecutionError>>()?,
        ))
    } else {
        Ok(EntityFilter::Child(Child {
            attr,
            entity_type: schema.input.entity_type(*type_name)?,
            filter,
            derived,
        }))
    }
}

/// Parses a list of GraphQL values into a vector of entity field values.
fn list_values(value: Value, filter_type: &str) -> Result<Vec<Value>, QueryExecutionError> {
    match value {
        Value::List(ref values) if !values.is_empty() => {
            // Check that all values in list are of the same type
            let root_discriminant = discriminant(&values[0]);
            values
                .iter()
                .map(|value| {
                    let current_discriminant = discriminant(value);
                    if root_discriminant == current_discriminant {
                        Ok(value.clone())
                    } else {
                        Err(QueryExecutionError::ListTypesError(
                            filter_type.to_string(),
                            vec![values[0].to_string(), value.to_string()],
                        ))
                    }
                })
                .collect::<Result<Vec<_>, _>>()
        }
        Value::List(ref values) if values.is_empty() => Ok(vec![]),
        _ => Err(QueryExecutionError::ListFilterError(
            filter_type.to_string(),
        )),
    }
}

enum OrderByValue {
    Direct(String),
    Child(String, String),
}

fn parse_order_by(enum_value: &String) -> Result<OrderByValue, QueryExecutionError> {
    let mut parts = enum_value.split("__");
    let first = parts.next().ok_or_else(|| {
        QueryExecutionError::ValueParseError(
            "Invalid order value".to_string(),
            enum_value.to_string(),
        )
    })?;
    let second = parts.next();

    Ok(match second {
        Some(second) => OrderByValue::Child(first.to_string(), second.to_string()),
        None => OrderByValue::Direct(first.to_string()),
    })
}

struct ObjectOrderDetails {
    entity_type: EntityType,
    join_attribute: Attribute,
    derived: bool,
}

struct InterfaceOrderDetails {
    entity_types: Vec<EntityType>,
    join_attribute: Attribute,
    derived: bool,
}

enum OrderByChild {
    Object(ObjectOrderDetails),
    Interface(InterfaceOrderDetails),
}

/// Parses GraphQL arguments into an field name to order by, if present.
fn build_order_by(
    entity: ObjectOrInterface,
    field: &a::Field,
    schema: &SchemaPair,
) -> Result<Option<(String, ValueType, Option<OrderByChild>)>, QueryExecutionError> {
    match field.argument_value("orderBy") {
        Some(r::Value::Enum(name)) => match parse_order_by(name)? {
            OrderByValue::Direct(name) => {
                let field = sast::get_field(entity, name.as_str()).ok_or_else(|| {
                    QueryExecutionError::EntityFieldError(entity.name().to_owned(), name.clone())
                })?;
                sast::get_field_value_type(&field.field_type)
                    .map(|value_type| Some((name.clone(), value_type, None)))
                    .map_err(|_| {
                        QueryExecutionError::OrderByNotSupportedError(
                            entity.name().to_owned(),
                            name.clone(),
                        )
                    })
            }
            OrderByValue::Child(parent_field_name, child_field_name) => {
                // Finds the field that connects the parent entity with the child entity.
                // In the case of an interface, we need to find the field on one of the types that implement the interface,
                // as the `@derivedFrom` directive is only allowed on object types.
                let field = match entity {
                    ObjectOrInterface::Object(_) => {
                        sast::get_field(entity, parent_field_name.as_str()).ok_or_else(|| {
                            QueryExecutionError::EntityFieldError(
                                entity.name().to_owned(),
                                parent_field_name.clone(),
                            )
                        })?
                    }
                    ObjectOrInterface::Interface(_) => {
                        let object_types =
                            schema.api.types_for_interface().get(entity.name()).ok_or(
                                QueryExecutionError::EntityFieldError(
                                    entity.name().to_owned(),
                                    parent_field_name.clone(),
                                ),
                            )?;

                        if let Some(first_entity) = object_types.first() {
                            sast::get_field(first_entity, parent_field_name.as_str()).ok_or_else(
                                || {
                                    QueryExecutionError::EntityFieldError(
                                        entity.name().to_owned(),
                                        parent_field_name.clone(),
                                    )
                                },
                            )?
                        } else {
                            Err(QueryExecutionError::EntityFieldError(
                                entity.name().to_owned(),
                                parent_field_name.clone(),
                            ))?
                        }
                    }
                };
                let derived = field.is_derived();
                let base_type = field.field_type.get_base_type();

                let child_entity = schema
                    .api
                    .object_or_interface(base_type)
                    .ok_or_else(|| QueryExecutionError::NamedTypeError(base_type.into()))?;
                let child_field = sast::get_field(child_entity, child_field_name.as_str())
                    .ok_or_else(|| {
                        QueryExecutionError::EntityFieldError(
                            child_entity.name().to_owned(),
                            child_field_name.clone(),
                        )
                    })?;

                let join_attribute = match derived {
                    true => sast::get_derived_from_field(child_entity, field)
                        .ok_or_else(|| {
                            QueryExecutionError::EntityFieldError(
                                entity.name().to_string(),
                                field.name.to_string(),
                            )
                        })?
                        .name
                        .to_string(),
                    false => parent_field_name,
                };

                let child = match child_entity {
                    ObjectOrInterface::Object(_) => OrderByChild::Object(ObjectOrderDetails {
                        entity_type: schema.input.entity_type(base_type)?,
                        join_attribute,
                        derived,
                    }),
                    ObjectOrInterface::Interface(interface) => {
                        let entity_types = schema
                            .api
                            .types_for_interface()
                            .get(&interface.name)
                            .map(|object_types| {
                                object_types
                                    .iter()
                                    .map(|object_type| schema.input.entity_type(object_type))
                                    .collect::<Result<Vec<EntityType>, _>>()
                            })
                            .ok_or(QueryExecutionError::AbstractTypeError(
                                "Interface not implemented by any object type".to_string(),
                            ))??;
                        OrderByChild::Interface(InterfaceOrderDetails {
                            entity_types,
                            join_attribute,
                            derived,
                        })
                    }
                };

                sast::get_field_value_type(&child_field.field_type)
                    .map(|value_type| Some((child_field_name.clone(), value_type, Some(child))))
                    .map_err(|_| {
                        QueryExecutionError::OrderByNotSupportedError(
                            child_entity.name().to_owned(),
                            child_field_name.clone(),
                        )
                    })
            }
        },
        _ => match field.argument_value("text") {
            Some(r::Value::Object(filter)) => build_fulltext_order_by_from_object(filter)
                .map(|order_by| order_by.map(|(attr, value)| (attr, value, None))),
            None => Ok(None),
            _ => Err(QueryExecutionError::InvalidFilterError),
        },
    }
}

fn build_fulltext_order_by_from_object(
    object: &Object,
) -> Result<Option<(String, ValueType)>, QueryExecutionError> {
    object.iter().next().map_or(
        Err(QueryExecutionError::FulltextQueryRequiresFilter),
        |(key, value)| {
            if let r::Value::String(_) = value {
                Ok(Some((key.to_string(), ValueType::String)))
            } else {
                Err(QueryExecutionError::FulltextQueryRequiresFilter)
            }
        },
    )
}

/// Parses GraphQL arguments into a EntityOrder, if present.
fn build_order_direction(field: &a::Field) -> Result<OrderDirection, QueryExecutionError> {
    Ok(field
        .argument_value("orderDirection")
        .map(|value| match value {
            r::Value::Enum(name) if name == "asc" => OrderDirection::Ascending,
            r::Value::Enum(name) if name == "desc" => OrderDirection::Descending,
            _ => OrderDirection::Ascending,
        })
        .unwrap_or(OrderDirection::Ascending))
}

/// Parses the subgraph ID from the ObjectType directives.
pub fn parse_subgraph_id<'a>(
    entity: impl Into<ObjectOrInterface<'a>>,
) -> Result<DeploymentHash, QueryExecutionError> {
    let entity = entity.into();
    let entity_name = entity.name();
    entity
        .directives()
        .iter()
        .find(|directive| directive.name == "subgraphId")
        .and_then(|directive| directive.arguments.iter().find(|(name, _)| name == "id"))
        .and_then(|(_, value)| match value {
            s::Value::String(id) => Some(id),
            _ => None,
        })
        .ok_or(())
        .and_then(|id| DeploymentHash::new(id).map_err(|_| ()))
        .map_err(|_| QueryExecutionError::SubgraphDeploymentIdError(entity_name.to_owned()))
}

/// Recursively collects entities involved in a query field as `(subgraph ID, name)` tuples.
pub(crate) fn collect_entities_from_query_field(
    input_schema: &InputSchema,
    schema: &ApiSchema,
    object_type: sast::ObjectType,
    field: &a::Field,
) -> Result<BTreeSet<SubscriptionFilter>, QueryExecutionError> {
    // Output entities
    let mut entities = HashSet::new();

    // List of objects/fields to visit next
    let mut queue = VecDeque::new();
    queue.push_back((object_type, field));

    while let Some((object_type, field)) = queue.pop_front() {
        // Check if the field exists on the object type
        if let Some(field_type) = sast::get_field(&object_type, &field.name) {
            // Check if the field type corresponds to a type definition (in a valid schema,
            // this should always be the case)
            if let Some(type_definition) = schema.get_type_definition_from_field(field_type) {
                // If the field's type definition is an object type, extract that type
                if let s::TypeDefinition::Object(object_type) = type_definition {
                    // Only collect whether the field's type has an @entity directive
                    if sast::get_object_type_directive(object_type, String::from("entity"))
                        .is_some()
                    {
                        // Obtain the subgraph ID from the object type
                        if let Ok(subgraph_id) = parse_subgraph_id(object_type) {
                            // Add the (subgraph_id, entity_name) tuple to the result set
                            entities.insert((subgraph_id, object_type.name.clone()));
                        }
                    }

                    // If the query field has a non-empty selection set, this means we
                    // need to recursively process it
                    let object_type = schema.object_type(object_type).into();
                    for sub_field in field.selection_set.fields_for(&object_type)? {
                        queue.push_back((object_type.cheap_clone(), sub_field))
                    }
                }
            }
        }
    }

    entities
        .into_iter()
        .map(|(id, entity_type)| {
            input_schema
                .entity_type(&entity_type)
                .map(|entity_type| SubscriptionFilter::Entities(id, entity_type))
        })
        .collect::<Result<_, _>>()
        .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use graph::{
        data::value::Object,
        prelude::{
            r, AttributeNames, DeploymentHash, EntityCollection, EntityFilter, EntityRange, Value,
            ValueType, BLOCK_NUMBER_MAX,
        },
        prelude::{
            s::{self, Directive, Field, InputValue, ObjectType, Type, Value as SchemaValue},
            EntityOrder,
        },
        schema::InputSchema,
    };
    use graphql_parser::Pos;
    use std::{collections::BTreeMap, iter::FromIterator, sync::Arc};

    use super::{a, build_query, SchemaPair};

    fn default_object() -> ObjectType {
        let subgraph_id_argument = (
            String::from("id"),
            s::Value::String("QmZ5dsusHwD1PEbx6L4dLCWkDsk1BLhrx9mPsGyPvTxPCM".to_string()),
        );
        let subgraph_id_directive = Directive {
            name: "subgraphId".to_string(),
            position: Pos::default(),
            arguments: vec![subgraph_id_argument],
        };
        let name_input_value = InputValue {
            position: Pos::default(),
            description: Some("name input".to_string()),
            name: "name".to_string(),
            value_type: Type::NamedType("String".to_string()),
            default_value: Some(SchemaValue::String("name".to_string())),
            directives: vec![],
        };
        let name_field = Field {
            position: Pos::default(),
            description: Some("name field".to_string()),
            name: "name".to_string(),
            arguments: vec![name_input_value.clone()],
            field_type: Type::NamedType("String".to_string()),
            directives: vec![],
        };
        let email_field = Field {
            position: Pos::default(),
            description: Some("email field".to_string()),
            name: "email".to_string(),
            arguments: vec![name_input_value],
            field_type: Type::NamedType("String".to_string()),
            directives: vec![],
        };

        ObjectType {
            position: Default::default(),
            description: None,
            name: "DefaultObject".to_string(),
            implements_interfaces: vec![],
            directives: vec![subgraph_id_directive],
            fields: vec![name_field, email_field],
        }
    }

    fn object(name: &str) -> ObjectType {
        ObjectType {
            name: name.to_owned(),
            ..default_object()
        }
    }

    fn field(name: &str, field_type: Type) -> Field {
        Field {
            position: Default::default(),
            description: None,
            name: name.to_owned(),
            arguments: vec![],
            field_type,
            directives: vec![],
        }
    }

    fn default_field() -> a::Field {
        let arguments = vec![
            ("first".to_string(), r::Value::Int(100.into())),
            ("skip".to_string(), r::Value::Int(0.into())),
        ];
        let obj_type = Arc::new(object("SomeType")).into();
        a::Field {
            position: Default::default(),
            alias: None,
            name: "aField".to_string(),
            arguments,
            directives: vec![],
            selection_set: a::SelectionSet::new(vec![obj_type]),
        }
    }

    fn default_field_with(arg_name: &str, arg_value: r::Value) -> a::Field {
        let mut field = default_field();
        field.arguments.push((arg_name.to_string(), arg_value));
        field
    }

    fn default_field_with_vec(args: Vec<(&str, r::Value)>) -> a::Field {
        let mut field = default_field();
        for (name, value) in args {
            field.arguments.push((name.to_string(), value));
        }
        field
    }

    fn build_default_schema() -> SchemaPair {
        const INPUT_SCHEMA: &str = r#"
        type Entity1 @entity { id: ID! }
        type Entity2 @entity { id: ID! }
        type DefaultObject @entity {
            id: ID!
            name: String
            email: String
        }
    "#;

        let id = DeploymentHash::new("id").unwrap();
        let input_schema = InputSchema::parse(INPUT_SCHEMA, id.clone()).unwrap();
        let api_schema = input_schema.api_schema().unwrap();

        SchemaPair {
            input: input_schema,
            api: Arc::new(api_schema),
        }
    }

    #[test]
    fn build_query_uses_the_entity_name() {
        let schema = build_default_schema();
        assert_eq!(
            build_query(
                &object("Entity1"),
                BLOCK_NUMBER_MAX,
                &default_field(),
                &BTreeMap::new(),
                std::u32::MAX,
                std::u32::MAX,
                Default::default(),
                &schema,
            )
            .unwrap()
            .collection,
            EntityCollection::All(vec![(
                schema.input.entity_type("Entity1").unwrap(),
                AttributeNames::All
            )])
        );
        assert_eq!(
            build_query(
                &object("Entity2"),
                BLOCK_NUMBER_MAX,
                &default_field(),
                &BTreeMap::new(),
                std::u32::MAX,
                std::u32::MAX,
                Default::default(),
                &schema,
            )
            .unwrap()
            .collection,
            EntityCollection::All(vec![(
                schema.input.entity_type("Entity2").unwrap(),
                AttributeNames::All
            )])
        );
    }

    #[test]
    fn build_query_yields_no_order_if_order_arguments_are_missing() {
        let schema = build_default_schema();
        assert_eq!(
            build_query(
                &default_object(),
                BLOCK_NUMBER_MAX,
                &default_field(),
                &BTreeMap::new(),
                std::u32::MAX,
                std::u32::MAX,
                Default::default(),
                &schema,
            )
            .unwrap()
            .order,
            EntityOrder::Default,
        );
    }

    #[test]
    fn build_query_parses_order_by_from_enum_values_correctly() {
        let schema = build_default_schema();
        let field = default_field_with("orderBy", r::Value::Enum("name".to_string()));
        assert_eq!(
            build_query(
                &default_object(),
                BLOCK_NUMBER_MAX,
                &field,
                &BTreeMap::new(),
                std::u32::MAX,
                std::u32::MAX,
                Default::default(),
                &schema,
            )
            .unwrap()
            .order,
            EntityOrder::Ascending("name".to_string(), ValueType::String)
        );

        let field = default_field_with("orderBy", r::Value::Enum("email".to_string()));
        assert_eq!(
            build_query(
                &default_object(),
                BLOCK_NUMBER_MAX,
                &field,
                &BTreeMap::new(),
                std::u32::MAX,
                std::u32::MAX,
                Default::default(),
                &schema,
            )
            .unwrap()
            .order,
            EntityOrder::Ascending("email".to_string(), ValueType::String)
        );
    }

    #[test]
    fn build_query_ignores_order_by_from_non_enum_values() {
        let schema = build_default_schema();
        let field = default_field_with("orderBy", r::Value::String("name".to_string()));
        assert_eq!(
            build_query(
                &default_object(),
                BLOCK_NUMBER_MAX,
                &field,
                &BTreeMap::new(),
                std::u32::MAX,
                std::u32::MAX,
                Default::default(),
                &schema,
            )
            .unwrap()
            .order,
            EntityOrder::Default
        );

        let field = default_field_with("orderBy", r::Value::String("email".to_string()));
        assert_eq!(
            build_query(
                &default_object(),
                BLOCK_NUMBER_MAX,
                &field,
                &BTreeMap::new(),
                std::u32::MAX,
                std::u32::MAX,
                Default::default(),
                &schema,
            )
            .unwrap()
            .order,
            EntityOrder::Default
        );
    }

    #[test]
    fn build_query_parses_order_direction_from_enum_values_correctly() {
        let schema = build_default_schema();
        let field = default_field_with_vec(vec![
            ("orderBy", r::Value::Enum("name".to_string())),
            ("orderDirection", r::Value::Enum("asc".to_string())),
        ]);
        assert_eq!(
            build_query(
                &default_object(),
                BLOCK_NUMBER_MAX,
                &field,
                &BTreeMap::new(),
                std::u32::MAX,
                std::u32::MAX,
                Default::default(),
                &schema,
            )
            .unwrap()
            .order,
            EntityOrder::Ascending("name".to_string(), ValueType::String)
        );

        let field = default_field_with_vec(vec![
            ("orderBy", r::Value::Enum("name".to_string())),
            ("orderDirection", r::Value::Enum("desc".to_string())),
        ]);
        assert_eq!(
            build_query(
                &default_object(),
                BLOCK_NUMBER_MAX,
                &field,
                &BTreeMap::new(),
                std::u32::MAX,
                std::u32::MAX,
                Default::default(),
                &schema,
            )
            .unwrap()
            .order,
            EntityOrder::Descending("name".to_string(), ValueType::String)
        );

        let field = default_field_with_vec(vec![
            ("orderBy", r::Value::Enum("name".to_string())),
            (
                "orderDirection",
                r::Value::Enum("descending...".to_string()),
            ),
        ]);
        assert_eq!(
            build_query(
                &default_object(),
                BLOCK_NUMBER_MAX,
                &field,
                &BTreeMap::new(),
                std::u32::MAX,
                std::u32::MAX,
                Default::default(),
                &schema
            )
            .unwrap()
            .order,
            EntityOrder::Ascending("name".to_string(), ValueType::String)
        );

        // No orderBy -> EntityOrder::Default
        let field = default_field_with(
            "orderDirection",
            r::Value::Enum("descending...".to_string()),
        );
        assert_eq!(
            build_query(
                &default_object(),
                BLOCK_NUMBER_MAX,
                &field,
                &BTreeMap::new(),
                std::u32::MAX,
                std::u32::MAX,
                Default::default(),
                &schema
            )
            .unwrap()
            .order,
            EntityOrder::Default
        );
    }

    #[test]
    fn build_query_yields_default_range_if_none_is_present() {
        let schema = build_default_schema();
        assert_eq!(
            build_query(
                &default_object(),
                BLOCK_NUMBER_MAX,
                &default_field(),
                &BTreeMap::new(),
                std::u32::MAX,
                std::u32::MAX,
                Default::default(),
                &schema
            )
            .unwrap()
            .range,
            EntityRange::first(100)
        );
    }

    #[test]
    fn build_query_yields_default_first_if_only_skip_is_present() {
        let schema = build_default_schema();
        let mut field = default_field();
        field.arguments = vec![("skip".to_string(), r::Value::Int(50))];

        assert_eq!(
            build_query(
                &default_object(),
                BLOCK_NUMBER_MAX,
                &field,
                &BTreeMap::new(),
                std::u32::MAX,
                std::u32::MAX,
                Default::default(),
                &schema
            )
            .unwrap()
            .range,
            EntityRange {
                first: Some(100),
                skip: 50,
            },
        );
    }

    #[test]
    fn build_query_yields_filters() {
        let schema = build_default_schema();
        let query_field = default_field_with(
            "where",
            r::Value::Object(Object::from_iter(vec![(
                "name_ends_with".into(),
                r::Value::String("ello".to_string()),
            )])),
        );
        assert_eq!(
            build_query(
                &ObjectType {
                    fields: vec![field("name", Type::NamedType("string".to_owned()))],
                    ..default_object()
                },
                BLOCK_NUMBER_MAX,
                &query_field,
                &BTreeMap::new(),
                std::u32::MAX,
                std::u32::MAX,
                Default::default(),
                &schema
            )
            .unwrap()
            .filter,
            Some(EntityFilter::And(vec![EntityFilter::EndsWith(
                "name".to_string(),
                Value::String("ello".to_string()),
            )]))
        )
    }

    #[test]
    fn build_query_yields_block_change_gte_filter() {
        let schema = build_default_schema();
        let query_field = default_field_with(
            "where",
            r::Value::Object(Object::from_iter(vec![(
                "_change_block".into(),
                r::Value::Object(Object::from_iter(vec![(
                    "number_gte".into(),
                    r::Value::Int(10),
                )])),
            )])),
        );
        assert_eq!(
            build_query(
                &ObjectType {
                    fields: vec![field("name", Type::NamedType("string".to_owned()))],
                    ..default_object()
                },
                BLOCK_NUMBER_MAX,
                &query_field,
                &BTreeMap::new(),
                std::u32::MAX,
                std::u32::MAX,
                Default::default(),
                &schema
            )
            .unwrap()
            .filter,
            Some(EntityFilter::And(vec![EntityFilter::ChangeBlockGte(10)]))
        )
    }
}
