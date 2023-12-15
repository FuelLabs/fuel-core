use graphql_parser::Pos;
use lazy_static::lazy_static;
use std::ops::Deref;
use std::str::FromStr;
use std::sync::Arc;

use crate::cheap_clone::CheapClone;
use crate::data::graphql::ext::DirectiveFinder;
use crate::data::graphql::{DirectiveExt, DocumentExt, ObjectOrInterface};
use crate::prelude::anyhow::anyhow;
use crate::prelude::{s, Error, ValueType};

pub enum FilterOp {
    Not,
    GreaterThan,
    LessThan,
    GreaterOrEqual,
    LessOrEqual,
    In,
    NotIn,
    Contains,
    ContainsNoCase,
    NotContains,
    NotContainsNoCase,
    StartsWith,
    StartsWithNoCase,
    NotStartsWith,
    NotStartsWithNoCase,
    EndsWith,
    EndsWithNoCase,
    NotEndsWith,
    NotEndsWithNoCase,
    Equal,
    Child,
    And,
    Or,
}

/// Split a "name_eq" style name into an attribute ("name") and a filter op (`Equal`).
pub fn parse_field_as_filter(key: &str) -> (String, FilterOp) {
    let (suffix, op) = match key {
        k if k.ends_with("_not") => ("_not", FilterOp::Not),
        k if k.ends_with("_gt") => ("_gt", FilterOp::GreaterThan),
        k if k.ends_with("_lt") => ("_lt", FilterOp::LessThan),
        k if k.ends_with("_gte") => ("_gte", FilterOp::GreaterOrEqual),
        k if k.ends_with("_lte") => ("_lte", FilterOp::LessOrEqual),
        k if k.ends_with("_not_in") => ("_not_in", FilterOp::NotIn),
        k if k.ends_with("_in") => ("_in", FilterOp::In),
        k if k.ends_with("_not_contains") => ("_not_contains", FilterOp::NotContains),
        k if k.ends_with("_not_contains_nocase") => {
            ("_not_contains_nocase", FilterOp::NotContainsNoCase)
        }
        k if k.ends_with("_contains") => ("_contains", FilterOp::Contains),
        k if k.ends_with("_contains_nocase") => ("_contains_nocase", FilterOp::ContainsNoCase),
        k if k.ends_with("_not_starts_with") => ("_not_starts_with", FilterOp::NotStartsWith),
        k if k.ends_with("_not_starts_with_nocase") => {
            ("_not_starts_with_nocase", FilterOp::NotStartsWithNoCase)
        }
        k if k.ends_with("_not_ends_with") => ("_not_ends_with", FilterOp::NotEndsWith),
        k if k.ends_with("_not_ends_with_nocase") => {
            ("_not_ends_with_nocase", FilterOp::NotEndsWithNoCase)
        }
        k if k.ends_with("_starts_with") => ("_starts_with", FilterOp::StartsWith),
        k if k.ends_with("_starts_with_nocase") => {
            ("_starts_with_nocase", FilterOp::StartsWithNoCase)
        }
        k if k.ends_with("_ends_with") => ("_ends_with", FilterOp::EndsWith),
        k if k.ends_with("_ends_with_nocase") => ("_ends_with_nocase", FilterOp::EndsWithNoCase),
        k if k.ends_with('_') => ("_", FilterOp::Child),
        k if k.eq("and") => ("and", FilterOp::And),
        k if k.eq("or") => ("or", FilterOp::Or),
        _ => ("", FilterOp::Equal),
    };

    return match op {
        FilterOp::And => (key.to_owned(), op),
        FilterOp::Or => (key.to_owned(), op),
        // Strip the operator suffix to get the attribute.
        _ => (key.trim_end_matches(suffix).to_owned(), op),
    };
}

/// An `ObjectType` with `Hash` and `Eq` derived from the name.
#[derive(Clone, Debug)]
pub struct ObjectType(Arc<s::ObjectType>);

impl Ord for ObjectType {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.name.cmp(&other.0.name)
    }
}

impl PartialOrd for ObjectType {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.0.name.cmp(&other.0.name))
    }
}

impl std::hash::Hash for ObjectType {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.name.hash(state)
    }
}

impl PartialEq for ObjectType {
    fn eq(&self, other: &Self) -> bool {
        self.0.name.eq(&other.0.name)
    }
}

impl Eq for ObjectType {}

impl From<Arc<s::ObjectType>> for ObjectType {
    fn from(object: Arc<s::ObjectType>) -> Self {
        ObjectType(object)
    }
}

impl<'a> From<&'a ObjectType> for ObjectOrInterface<'a> {
    fn from(cond: &'a ObjectType) -> Self {
        ObjectOrInterface::Object(cond.0.as_ref())
    }
}

impl Deref for ObjectType {
    type Target = s::ObjectType;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl CheapClone for ObjectType {}

impl ObjectType {
    pub fn name(&self) -> &str {
        &self.0.name
    }
}

/// Returns all type definitions in the schema.
pub fn get_type_definitions(schema: &s::Document) -> Vec<&s::TypeDefinition> {
    schema
        .definitions
        .iter()
        .filter_map(|d| match d {
            s::Definition::TypeDefinition(typedef) => Some(typedef),
            _ => None,
        })
        .collect()
}

/// Returns the object type with the given name.
pub fn get_object_type_mut<'a>(
    schema: &'a mut s::Document,
    name: &str,
) -> Option<&'a mut s::ObjectType> {
    use graphql_parser::schema::TypeDefinition::*;

    get_named_type_definition_mut(schema, name).and_then(|type_def| match type_def {
        Object(object_type) => Some(object_type),
        _ => None,
    })
}

/// Returns the interface type with the given name.
pub fn get_interface_type_mut<'a>(
    schema: &'a mut s::Document,
    name: &str,
) -> Option<&'a mut s::InterfaceType> {
    use graphql_parser::schema::TypeDefinition::*;

    get_named_type_definition_mut(schema, name).and_then(|type_def| match type_def {
        Interface(interface_type) => Some(interface_type),
        _ => None,
    })
}

/// Returns the type of a field of an object type.
pub fn get_field<'a>(
    object_type: impl Into<ObjectOrInterface<'a>>,
    name: &str,
) -> Option<&'a s::Field> {
    lazy_static! {
        pub static ref TYPENAME_FIELD: s::Field = s::Field {
            position: Pos::default(),
            description: None,
            name: "__typename".to_owned(),
            field_type: s::Type::NonNullType(Box::new(s::Type::NamedType("String".to_owned()))),
            arguments: vec![],
            directives: vec![],
        };
    }

    if name == TYPENAME_FIELD.name {
        Some(&TYPENAME_FIELD)
    } else {
        object_type
            .into()
            .fields()
            .iter()
            .find(|field| field.name == name)
    }
}

/// Returns the value type for a GraphQL field type.
pub fn get_field_value_type(field_type: &s::Type) -> Result<ValueType, Error> {
    match field_type {
        s::Type::NamedType(ref name) => ValueType::from_str(name),
        s::Type::NonNullType(inner) => get_field_value_type(inner),
        s::Type::ListType(_) => Err(anyhow!("Only scalar values are supported in this context")),
    }
}

/// Returns the value type for a GraphQL field type.
pub fn get_field_name(field_type: &s::Type) -> String {
    match field_type {
        s::Type::NamedType(name) => name.to_string(),
        s::Type::NonNullType(inner) => get_field_name(inner),
        s::Type::ListType(inner) => get_field_name(inner),
    }
}

/// Returns a mutable version of the type with the given name.
fn get_named_type_definition_mut<'a>(
    schema: &'a mut s::Document,
    name: &str,
) -> Option<&'a mut s::TypeDefinition> {
    schema
        .definitions
        .iter_mut()
        .filter_map(|def| match def {
            s::Definition::TypeDefinition(typedef) => Some(typedef),
            _ => None,
        })
        .find(|typedef| get_type_name(typedef) == name)
}

/// Returns the name of a type.
pub fn get_type_name(t: &s::TypeDefinition) -> &str {
    match t {
        s::TypeDefinition::Enum(t) => &t.name,
        s::TypeDefinition::InputObject(t) => &t.name,
        s::TypeDefinition::Interface(t) => &t.name,
        s::TypeDefinition::Object(t) => &t.name,
        s::TypeDefinition::Scalar(t) => &t.name,
        s::TypeDefinition::Union(t) => &t.name,
    }
}

/// Returns the argument definitions for a field of an object type.
pub fn get_argument_definitions<'a>(
    object_type: impl Into<ObjectOrInterface<'a>>,
    name: &str,
) -> Option<&'a Vec<s::InputValue>> {
    lazy_static! {
        pub static ref NAME_ARGUMENT: Vec<s::InputValue> = vec![s::InputValue {
            position: Pos::default(),
            description: None,
            name: "name".to_owned(),
            value_type: s::Type::NonNullType(Box::new(s::Type::NamedType("String".to_owned()))),
            default_value: None,
            directives: vec![],
        }];
    }

    // Introspection: `__type(name: String!): __Type`
    if name == "__type" {
        Some(&NAME_ARGUMENT)
    } else {
        get_field(object_type, name).map(|field| &field.arguments)
    }
}

/// Returns the type definition for a type.
pub fn get_type_definition_from_type<'a>(
    schema: &'a s::Document,
    t: &s::Type,
) -> Option<&'a s::TypeDefinition> {
    match t {
        s::Type::NamedType(name) => schema.get_named_type(name),
        s::Type::ListType(inner) => get_type_definition_from_type(schema, inner),
        s::Type::NonNullType(inner) => get_type_definition_from_type(schema, inner),
    }
}

/// Looks up a directive in a object type, if it is provided.
pub fn get_object_type_directive(
    object_type: &s::ObjectType,
    name: String,
) -> Option<&s::Directive> {
    object_type
        .directives
        .iter()
        .find(|directive| directive.name == name)
}

// Returns true if the given type is a non-null type.
pub fn is_non_null_type(t: &s::Type) -> bool {
    match t {
        s::Type::NonNullType(_) => true,
        _ => false,
    }
}

/// Returns true if the given type is an input type.
///
/// Uses the algorithm outlined on
/// https://facebook.github.io/graphql/draft/#IsInputType().
pub fn is_input_type(schema: &s::Document, t: &s::Type) -> bool {
    match t {
        s::Type::NamedType(name) => {
            let named_type = schema.get_named_type(name);
            named_type.map_or(false, |type_def| match type_def {
                s::TypeDefinition::Scalar(_)
                | s::TypeDefinition::Enum(_)
                | s::TypeDefinition::InputObject(_) => true,
                _ => false,
            })
        }
        s::Type::ListType(inner) => is_input_type(schema, inner),
        s::Type::NonNullType(inner) => is_input_type(schema, inner),
    }
}

pub fn is_entity_type(schema: &s::Document, t: &s::Type) -> bool {
    match t {
        s::Type::NamedType(name) => schema
            .get_named_type(name)
            .map_or(false, is_entity_type_definition),
        s::Type::ListType(inner_type) => is_entity_type(schema, inner_type),
        s::Type::NonNullType(inner_type) => is_entity_type(schema, inner_type),
    }
}

pub fn is_entity_type_definition(type_def: &s::TypeDefinition) -> bool {
    match type_def {
        // Entity types are obvious
        s::TypeDefinition::Object(object_type) => {
            get_object_type_directive(object_type, String::from("entity")).is_some()
        }

        // For now, we'll assume that only entities can implement interfaces;
        // thus, any interface type definition is automatically an entity type
        s::TypeDefinition::Interface(_) => true,

        // Everything else (unions, scalars, enums) are not considered entity
        // types for now
        _ => false,
    }
}

pub fn is_list_or_non_null_list_field(field: &s::Field) -> bool {
    match &field.field_type {
        s::Type::ListType(_) => true,
        s::Type::NonNullType(inner_type) => match inner_type.deref() {
            s::Type::ListType(_) => true,
            _ => false,
        },
        _ => false,
    }
}

fn unpack_type<'a>(schema: &'a s::Document, t: &s::Type) -> Option<&'a s::TypeDefinition> {
    match t {
        s::Type::NamedType(name) => schema.get_named_type(name),
        s::Type::ListType(inner_type) => unpack_type(schema, inner_type),
        s::Type::NonNullType(inner_type) => unpack_type(schema, inner_type),
    }
}

pub fn get_referenced_entity_type<'a>(
    schema: &'a s::Document,
    field: &s::Field,
) -> Option<&'a s::TypeDefinition> {
    unpack_type(schema, &field.field_type).filter(|ty| is_entity_type_definition(ty))
}

/// If the field has a `@derivedFrom(field: "foo")` directive, obtain the
/// name of the field (e.g. `"foo"`)
pub fn get_derived_from_directive(field_definition: &s::Field) -> Option<&s::Directive> {
    field_definition.find_directive("derivedFrom")
}

pub fn get_derived_from_field<'a>(
    object_type: impl Into<ObjectOrInterface<'a>>,
    field_definition: &'a s::Field,
) -> Option<&'a s::Field> {
    get_derived_from_directive(field_definition)
        .and_then(|directive| directive.argument("field"))
        .and_then(|value| match value {
            s::Value::String(s) => Some(s),
            _ => None,
        })
        .and_then(|derived_from_field_name| get_field(object_type, derived_from_field_name))
}

pub fn is_list(field_type: &s::Type) -> bool {
    match field_type {
        s::Type::NamedType(_) => false,
        s::Type::NonNullType(inner) => is_list(inner),
        s::Type::ListType(_) => true,
    }
}

#[test]
fn entity_validation() {
    use crate::data::store;
    use crate::entity;
    use crate::prelude::{DeploymentHash, Entity};
    use crate::schema::{EntityType, InputSchema};

    const DOCUMENT: &str = "
    enum Color { red, yellow, blue }
    interface Stuff { id: ID!, name: String! }
    type Cruft @entity {
        id: ID!,
        thing: Thing!
    }
    type Thing @entity {
        id: ID!,
        name: String!,
        favorite_color: Color,
        stuff: Stuff,
        things: [Thing!]!
        # Make sure we do not validate derived fields; it's ok
        # to store a thing with a null Cruft
        cruft: Cruft! @derivedFrom(field: \"thing\")
    }";

    lazy_static! {
        static ref SUBGRAPH: DeploymentHash = DeploymentHash::new("doesntmatter").unwrap();
        static ref SCHEMA: InputSchema = InputSchema::raw(DOCUMENT, "doesntmatter");
        static ref THING_TYPE: EntityType = SCHEMA.entity_type("Thing").unwrap();
    }

    fn make_thing(name: &str) -> Entity {
        entity! { SCHEMA => id: name, name: name, stuff: "less", favorite_color: "red", things: store::Value::List(vec![]) }
    }

    fn check(thing: Entity, errmsg: &str) {
        let id = thing.id();
        let key = THING_TYPE.key(id.clone());

        let err = thing.validate(&key);
        if errmsg.is_empty() {
            assert!(
                err.is_ok(),
                "checking entity {}: expected ok but got {}",
                id,
                err.unwrap_err()
            );
        } else if let Err(e) = err {
            assert_eq!(errmsg, e.to_string(), "checking entity {}", id);
        } else {
            panic!(
                "Expected error `{}` but got ok when checking entity {}",
                errmsg, id
            );
        }
    }

    let mut thing = make_thing("t1");
    thing
        .set("things", store::Value::from(vec!["thing1", "thing2"]))
        .unwrap();
    check(thing, "");

    let thing = make_thing("t2");
    check(thing, "");

    let mut thing = make_thing("t3");
    thing.remove("name");
    check(
        thing,
        "Entity Thing[t3]: missing value for non-nullable field `name`",
    );

    let mut thing = make_thing("t4");
    thing.remove("things");
    check(
        thing,
        "Entity Thing[t4]: missing value for non-nullable field `things`",
    );

    let mut thing = make_thing("t5");
    thing.set("name", store::Value::Int(32)).unwrap();
    check(
        thing,
        "Entity Thing[t5]: the value `32` for field `name` must \
         have type String! but has type Int",
    );

    let mut thing = make_thing("t6");
    thing
        .set(
            "things",
            store::Value::List(vec!["thing1".into(), 17.into()]),
        )
        .unwrap();
    check(
        thing,
        "Entity Thing[t6]: field `things` is of type [Thing!]!, \
         but the value `[thing1, 17]` contains a Int at index 1",
    );

    let mut thing = make_thing("t7");
    thing.remove("favorite_color");
    thing.remove("stuff");
    check(thing, "");

    let mut thing = make_thing("t8");
    thing.set("cruft", "wat").unwrap();
    check(
        thing,
        "Entity Thing[t8]: field `cruft` is derived and cannot be set",
    );
}
