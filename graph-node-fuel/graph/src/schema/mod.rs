use crate::data::graphql::ext::DocumentExt;
use crate::data::subgraph::DeploymentHash;
use crate::prelude::{anyhow, s};

use anyhow::Error;
use graphql_parser::{self, Pos};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use std::collections::BTreeMap;
use std::fmt;
use std::iter::FromIterator;

/// Generate full-fledged API schemas from existing GraphQL schemas.
mod api;

/// Utilities for working with GraphQL schema ASTs.
pub mod ast;

mod entity_key;
mod entity_type;
mod fulltext;
mod input_schema;

pub use api::{is_introspection_field, APISchemaError, INTROSPECTION_QUERY_TYPE};

pub use api::{ApiSchema, ErrorPolicy};
pub use entity_key::EntityKey;
pub use entity_type::{AsEntityTypeName, EntityType};
pub use fulltext::{FulltextAlgorithm, FulltextConfig, FulltextDefinition, FulltextLanguage};
pub use input_schema::{Field, InputSchema, InterfaceType, ObjectType};

pub const SCHEMA_TYPE_NAME: &str = "_Schema_";
pub const INTROSPECTION_SCHEMA_FIELD_NAME: &str = "__schema";

pub const META_FIELD_TYPE: &str = "_Meta_";
pub const META_FIELD_NAME: &str = "_meta";

pub const INTROSPECTION_TYPE_FIELD_NAME: &str = "__type";

pub const BLOCK_FIELD_TYPE: &str = "_Block_";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Strings(Vec<String>);

impl fmt::Display for Strings {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        let s = self.0.join(", ");
        write!(f, "{}", s)
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SchemaValidationError {
    #[error("Interface `{0}` not defined")]
    InterfaceUndefined(String),

    #[error("@entity directive missing on the following types: `{0}`")]
    EntityDirectivesMissing(Strings),

    #[error(
        "Entity type `{0}` does not satisfy interface `{1}` because it is missing \
         the following fields: {2}"
    )]
    InterfaceFieldsMissing(String, String, Strings), // (type, interface, missing_fields)
    #[error("Implementors of interface `{0}` use different id types `{1}`. They must all use the same type")]
    InterfaceImplementorsMixId(String, String),
    #[error("Field `{1}` in type `{0}` has invalid @derivedFrom: {2}")]
    InvalidDerivedFrom(String, String, String), // (type, field, reason)
    #[error("The following type names are reserved: `{0}`")]
    UsageOfReservedTypes(Strings),
    #[error("_Schema_ type is only for @fulltext and must not have any fields")]
    SchemaTypeWithFields,
    #[error("The _Schema_ type only allows @fulltext directives")]
    InvalidSchemaTypeDirectives,
    #[error("Type `{0}`, field `{1}`: type `{2}` is not defined")]
    FieldTypeUnknown(String, String, String), // (type_name, field_name, field_type)
    #[error("Imported type `{0}` does not exist in the `{1}` schema")]
    ImportedTypeUndefined(String, String), // (type_name, schema)
    #[error("Fulltext directive name undefined")]
    FulltextNameUndefined,
    #[error("Fulltext directive name overlaps with type: {0}")]
    FulltextNameConflict(String),
    #[error("Fulltext directive name overlaps with an existing entity field or a top-level query field: {0}")]
    FulltextNameCollision(String),
    #[error("Fulltext language is undefined")]
    FulltextLanguageUndefined,
    #[error("Fulltext language is invalid: {0}")]
    FulltextLanguageInvalid(String),
    #[error("Fulltext algorithm is undefined")]
    FulltextAlgorithmUndefined,
    #[error("Fulltext algorithm is invalid: {0}")]
    FulltextAlgorithmInvalid(String),
    #[error("Fulltext include is invalid")]
    FulltextIncludeInvalid,
    #[error("Fulltext directive requires an 'include' list")]
    FulltextIncludeUndefined,
    #[error("Fulltext 'include' list must contain an object")]
    FulltextIncludeObjectMissing,
    #[error(
        "Fulltext 'include' object must contain 'entity' (String) and 'fields' (List) attributes"
    )]
    FulltextIncludeEntityMissingOrIncorrectAttributes,
    #[error("Fulltext directive includes an entity not found on the subgraph schema")]
    FulltextIncludedEntityNotFound,
    #[error("Fulltext include field must have a 'name' attribute")]
    FulltextIncludedFieldMissingRequiredProperty,
    #[error("Fulltext entity field, {0}, not found or not a string")]
    FulltextIncludedFieldInvalid(String),
    #[error("Type {0} is missing an `id` field")]
    IdFieldMissing(String),
    #[error("{0}")]
    IllegalIdType(String),
}

/// A validated and preprocessed GraphQL schema for a subgraph.
#[derive(Clone, Debug, PartialEq)]
pub struct Schema {
    pub id: DeploymentHash,
    pub document: s::Document,

    // Maps type name to implemented interfaces.
    pub interfaces_for_type: BTreeMap<String, Vec<s::InterfaceType>>,

    // Maps an interface name to the list of entities that implement it.
    pub types_for_interface: BTreeMap<String, Vec<s::ObjectType>>,
}

impl Schema {
    /// Create a new schema. The document must already have been validated
    //
    // TODO: The way some validation is expected to be done beforehand, and
    // some is done here makes it incredibly murky whether a `Schema` is
    // fully validated. The code should be changed to make sure that a
    // `Schema` is always fully valid
    pub fn new(id: DeploymentHash, document: s::Document) -> Result<Self, SchemaValidationError> {
        let (interfaces_for_type, types_for_interface) = Self::collect_interfaces(&document)?;

        let mut schema = Schema {
            id: id.clone(),
            document,
            interfaces_for_type,
            types_for_interface,
        };

        schema.add_subgraph_id_directives(id);

        Ok(schema)
    }

    fn collect_interfaces(
        document: &s::Document,
    ) -> Result<
        (
            BTreeMap<String, Vec<s::InterfaceType>>,
            BTreeMap<String, Vec<s::ObjectType>>,
        ),
        SchemaValidationError,
    > {
        // Initialize with an empty vec for each interface, so we don't
        // miss interfaces that have no implementors.
        let mut types_for_interface =
            BTreeMap::from_iter(document.definitions.iter().filter_map(|d| match d {
                s::Definition::TypeDefinition(s::TypeDefinition::Interface(t)) => {
                    Some((t.name.to_string(), vec![]))
                }
                _ => None,
            }));
        let mut interfaces_for_type = BTreeMap::<_, Vec<_>>::new();

        for object_type in document.get_object_type_definitions() {
            for implemented_interface in &object_type.implements_interfaces {
                let interface_type = document
                    .definitions
                    .iter()
                    .find_map(|def| match def {
                        s::Definition::TypeDefinition(s::TypeDefinition::Interface(i))
                            if i.name.eq(implemented_interface) =>
                        {
                            Some(i.clone())
                        }
                        _ => None,
                    })
                    .ok_or_else(|| {
                        SchemaValidationError::InterfaceUndefined(implemented_interface.clone())
                    })?;

                Self::validate_interface_implementation(object_type, &interface_type)?;

                interfaces_for_type
                    .entry(object_type.name.to_owned())
                    .or_default()
                    .push(interface_type);
                types_for_interface
                    .get_mut(implemented_interface)
                    .unwrap()
                    .push(object_type.clone());
            }
        }

        Ok((interfaces_for_type, types_for_interface))
    }

    pub fn parse(raw: &str, id: DeploymentHash) -> Result<Self, Error> {
        let document = graphql_parser::parse_schema(raw)?.into_static();

        Schema::new(id, document).map_err(Into::into)
    }

    /// Returned map has one an entry for each interface in the schema.
    pub fn types_for_interface(&self) -> &BTreeMap<String, Vec<s::ObjectType>> {
        &self.types_for_interface
    }

    /// Returns `None` if the type implements no interfaces.
    pub fn interfaces_for_type(&self, type_name: &str) -> Option<&Vec<s::InterfaceType>> {
        self.interfaces_for_type.get(type_name)
    }

    // Adds a @subgraphId(id: ...) directive to object/interface/enum types in the schema.
    pub fn add_subgraph_id_directives(&mut self, id: DeploymentHash) {
        for definition in self.document.definitions.iter_mut() {
            let subgraph_id_argument = (String::from("id"), s::Value::String(id.to_string()));

            let subgraph_id_directive = s::Directive {
                name: "subgraphId".to_string(),
                position: Pos::default(),
                arguments: vec![subgraph_id_argument],
            };

            if let s::Definition::TypeDefinition(ref mut type_definition) = definition {
                let (name, directives) = match type_definition {
                    s::TypeDefinition::Object(object_type) => {
                        (&object_type.name, &mut object_type.directives)
                    }
                    s::TypeDefinition::Interface(interface_type) => {
                        (&interface_type.name, &mut interface_type.directives)
                    }
                    s::TypeDefinition::Enum(enum_type) => {
                        (&enum_type.name, &mut enum_type.directives)
                    }
                    s::TypeDefinition::Scalar(scalar_type) => {
                        (&scalar_type.name, &mut scalar_type.directives)
                    }
                    s::TypeDefinition::InputObject(input_object_type) => {
                        (&input_object_type.name, &mut input_object_type.directives)
                    }
                    s::TypeDefinition::Union(union_type) => {
                        (&union_type.name, &mut union_type.directives)
                    }
                };

                if !name.eq(SCHEMA_TYPE_NAME)
                    && !directives
                        .iter()
                        .any(|directive| directive.name.eq("subgraphId"))
                {
                    directives.push(subgraph_id_directive);
                }
            };
        }
    }

    /// Validate that `object` implements `interface`.
    fn validate_interface_implementation(
        object: &s::ObjectType,
        interface: &s::InterfaceType,
    ) -> Result<(), SchemaValidationError> {
        // Check that all fields in the interface exist in the object with same name and type.
        let mut missing_fields = vec![];
        for i in &interface.fields {
            if !object
                .fields
                .iter()
                .any(|o| o.name.eq(&i.name) && o.field_type.eq(&i.field_type))
            {
                missing_fields.push(i.to_string().trim().to_owned());
            }
        }
        if !missing_fields.is_empty() {
            Err(SchemaValidationError::InterfaceFieldsMissing(
                object.name.clone(),
                interface.name.clone(),
                Strings(missing_fields),
            ))
        } else {
            Ok(())
        }
    }

    fn subgraph_schema_object_type(&self) -> Option<&s::ObjectType> {
        self.document
            .get_object_type_definitions()
            .into_iter()
            .find(|object_type| object_type.name.eq(SCHEMA_TYPE_NAME))
    }
}

#[test]
fn non_existing_interface() {
    let schema = "type Foo implements Bar @entity { foo: Int }";
    let res = Schema::parse(schema, DeploymentHash::new("dummy").unwrap());
    let error = res
        .unwrap_err()
        .downcast::<SchemaValidationError>()
        .unwrap();
    assert_eq!(
        error,
        SchemaValidationError::InterfaceUndefined("Bar".to_owned())
    );
}

#[test]
fn invalid_interface_implementation() {
    let schema = "
        interface Foo {
            x: Int,
            y: Int
        }

        type Bar implements Foo @entity {
            x: Boolean
        }
    ";
    let res = Schema::parse(schema, DeploymentHash::new("dummy").unwrap());
    assert_eq!(
        res.unwrap_err().to_string(),
        "Entity type `Bar` does not satisfy interface `Foo` because it is missing \
         the following fields: x: Int, y: Int",
    );
}
