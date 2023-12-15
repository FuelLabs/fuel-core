use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use anyhow::{anyhow, Error};
use store::Entity;

use crate::bail;
use crate::cheap_clone::CheapClone;
use crate::components::store::LoadRelatedRequest;
use crate::data::graphql::ext::DirectiveFinder;
use crate::data::graphql::{DirectiveExt, DocumentExt, ObjectTypeExt, TypeExt, ValueExt};
use crate::data::store::{
    self, EntityValidationError, IdType, IntoEntityIterator, TryIntoEntityIterator, ValueType, ID,
};
use crate::data::value::Word;
use crate::prelude::q::Value;
use crate::prelude::{s, DeploymentHash};
use crate::schema::api::api_schema;
use crate::util::intern::{Atom, AtomPool};

use super::fulltext::FulltextDefinition;
use super::{ApiSchema, AsEntityTypeName, EntityType, Schema, SCHEMA_TYPE_NAME};

/// The name of the PoI entity type
pub(crate) const POI_OBJECT: &str = "Poi$";
/// The name of the digest attribute of POI entities
const POI_DIGEST: &str = "digest";

/// The internal representation of a subgraph schema, i.e., the
/// `schema.graphql` file that is part of a subgraph. Any code that deals
/// with writing a subgraph should use this struct. Code that deals with
/// querying subgraphs will instead want to use an `ApiSchema` which can be
/// generated with the `api_schema` method on `InputSchema`
///
/// There's no need to put this into an `Arc`, since `InputSchema` already
/// does that internally and is `CheapClone`
#[derive(Clone, Debug, PartialEq)]
pub struct InputSchema {
    inner: Arc<Inner>,
}

#[derive(Debug, PartialEq)]
enum TypeKind {
    MutableObject(ObjectType),
    ImmutableObject(ObjectType),
    Interface(InterfaceType),
}

impl TypeKind {
    fn is_object(&self) -> bool {
        match self {
            TypeKind::MutableObject(_) | TypeKind::ImmutableObject(_) => true,
            TypeKind::Interface(_) => false,
        }
    }

    fn is_interface(&self) -> bool {
        match self {
            TypeKind::MutableObject(_) | TypeKind::ImmutableObject(_) => false,
            TypeKind::Interface(_) => true,
        }
    }

    fn id_type(&self) -> Option<IdType> {
        match self {
            TypeKind::MutableObject(obj_type) | TypeKind::ImmutableObject(obj_type) => {
                Some(obj_type.id_type)
            }
            TypeKind::Interface(intf_type) => Some(intf_type.id_type),
        }
    }

    fn fields(&self) -> impl Iterator<Item = &Field> {
        match self {
            TypeKind::MutableObject(obj_type) | TypeKind::ImmutableObject(obj_type) => {
                obj_type.fields.iter()
            }
            TypeKind::Interface(intf_type) => intf_type.fields.iter(),
        }
    }

    fn name(&self) -> Atom {
        match self {
            TypeKind::MutableObject(obj_type) | TypeKind::ImmutableObject(obj_type) => {
                obj_type.name
            }
            TypeKind::Interface(intf_type) => intf_type.name,
        }
    }
}

#[derive(Debug, PartialEq)]
struct TypeInfo {
    kind: TypeKind,
    fields: Box<[Atom]>,
}

impl TypeInfo {
    fn new(pool: &AtomPool, kind: TypeKind) -> Self {
        // The `unwrap` of `lookup` is safe because the pool was just
        // constructed against the underlying schema
        let fields = kind
            .fields()
            .map(|field| pool.lookup(&field.name).unwrap())
            .collect::<Vec<_>>()
            .into_boxed_slice();
        Self { kind, fields }
    }

    fn name(&self) -> Atom {
        self.kind.name()
    }

    fn for_object(schema: &Schema, pool: &AtomPool, obj_type: &s::ObjectType) -> Self {
        let shared_interfaces: Vec<_> = match schema.interfaces_for_type(&obj_type.name) {
            Some(intfs) => {
                let mut shared_interfaces: Vec<_> = intfs
                    .iter()
                    .flat_map(|intf| &schema.types_for_interface[&intf.name])
                    .filter(|other| other.name != obj_type.name)
                    .map(|obj_type| pool.lookup(&obj_type.name).unwrap())
                    .collect();
                shared_interfaces.sort();
                shared_interfaces.dedup();
                shared_interfaces
            }
            None => Vec::new(),
        };
        let object_type =
            ObjectType::new(schema, pool, obj_type, shared_interfaces.into_boxed_slice());
        let kind = if obj_type.is_immutable() {
            TypeKind::ImmutableObject(object_type)
        } else {
            TypeKind::MutableObject(object_type)
        };
        Self::new(pool, kind)
    }

    fn for_interface(schema: &Schema, pool: &AtomPool, intf_type: &s::InterfaceType) -> Self {
        static EMPTY_VEC: [s::ObjectType; 0] = [];
        let implementers = schema
            .types_for_interface
            .get(&intf_type.name)
            .map(|impls| impls.as_slice())
            .unwrap_or_else(|| EMPTY_VEC.as_slice());
        let intf_type = InterfaceType::new(schema, pool, intf_type, implementers);
        Self::new(pool, TypeKind::Interface(intf_type))
    }

    fn for_poi(pool: &AtomPool) -> Self {
        // The way we handle the PoI type is a bit of a hack. We pretend
        // it's an object type, but trying to look up the `s::ObjectType`
        // for it will turn up nothing.
        // See also https://github.com/graphprotocol/graph-node/issues/4873
        let fields =
            vec![pool.lookup(&ID).unwrap(), pool.lookup(POI_DIGEST).unwrap()].into_boxed_slice();
        Self {
            kind: TypeKind::MutableObject(ObjectType::for_poi(pool)),
            fields,
        }
    }

    fn interfaces(&self) -> impl Iterator<Item = &str> {
        const NO_INTF: [Word; 0] = [];
        let interfaces = match &self.kind {
            TypeKind::MutableObject(obj_type) | TypeKind::ImmutableObject(obj_type) => {
                &obj_type.interfaces
            }
            TypeKind::Interface(_) => NO_INTF.as_slice(),
        };
        interfaces.iter().map(|interface| interface.as_str())
    }

    fn object_type(&self) -> Option<&ObjectType> {
        match &self.kind {
            TypeKind::MutableObject(obj_type) | TypeKind::ImmutableObject(obj_type) => {
                Some(obj_type)
            }
            TypeKind::Interface(_) => None,
        }
    }
}

#[derive(PartialEq, Debug)]
pub struct Field {
    pub name: Word,
    pub field_type: s::Type,
    pub value_type: ValueType,
    pub is_derived: bool,
}

impl Field {
    pub fn new(schema: &Schema, name: &str, field_type: &s::Type, is_derived: bool) -> Self {
        let value_type = Self::scalar_value_type(&schema, field_type);
        Self {
            name: Word::from(name),
            field_type: field_type.clone(),
            value_type,
            is_derived,
        }
    }

    fn scalar_value_type(schema: &Schema, field_type: &s::Type) -> ValueType {
        use s::TypeDefinition as t;
        match field_type {
            s::Type::NamedType(name) => name.parse::<ValueType>().unwrap_or_else(|_| {
                match schema.document.get_named_type(name) {
                    Some(t::Object(obj_type)) => {
                        let id = obj_type.field(&*ID).expect("all object types have an id");
                        Self::scalar_value_type(schema, &id.field_type)
                    }
                    Some(t::Interface(intf)) => {
                        // Validation checks that all implementors of an
                        // interface use the same type for `id`. It is
                        // therefore enough to use the id type of one of
                        // the implementors
                        match schema
                            .types_for_interface
                            .get(&intf.name)
                            .expect("interface type names are known")
                            .first()
                        {
                            None => {
                                // Nothing is implementing this interface; we assume it's of type string
                                // see also: id-type-for-unimplemented-interfaces
                                ValueType::String
                            }
                            Some(obj_type) => {
                                let id = obj_type.field(&*ID).expect("all object types have an id");
                                Self::scalar_value_type(schema, &id.field_type)
                            }
                        }
                    }
                    Some(t::Enum(_)) => ValueType::String,
                    Some(t::Scalar(_)) => unreachable!("user-defined scalars are not used"),
                    Some(t::Union(_)) => unreachable!("unions are not used"),
                    Some(t::InputObject(_)) => unreachable!("inputObjects are not used"),
                    None => unreachable!("names of field types have been validated"),
                }
            }),
            s::Type::NonNullType(inner) => Self::scalar_value_type(schema, inner),
            s::Type::ListType(inner) => Self::scalar_value_type(schema, inner),
        }
    }
}

#[derive(PartialEq, Debug)]
pub struct ObjectType {
    pub name: Atom,
    pub id_type: IdType,
    pub fields: Box<[Field]>,
    interfaces: Box<[Word]>,
    shared_interfaces: Box<[Atom]>,
}

impl ObjectType {
    fn new(
        schema: &Schema,
        pool: &AtomPool,
        object_type: &s::ObjectType,
        shared_interfaces: Box<[Atom]>,
    ) -> Self {
        let id_type = IdType::try_from(object_type).expect("validation caught any issues here");
        let fields = object_type
            .fields
            .iter()
            .map(|field| {
                let is_derived = field.is_derived();
                Field::new(schema, &field.name, &field.field_type, is_derived)
            })
            .collect();
        let interfaces = object_type
            .implements_interfaces
            .iter()
            .map(|intf| Word::from(intf.to_owned()))
            .collect();
        let name = pool
            .lookup(&object_type.name)
            .expect("object type names have been interned");
        Self {
            name,
            fields,
            id_type,
            interfaces,
            shared_interfaces,
        }
    }

    fn for_poi(pool: &AtomPool) -> Self {
        let fields = vec![
            Field {
                name: ID.clone(),
                field_type: s::Type::NamedType("ID".to_string()),
                value_type: ValueType::String,
                is_derived: false,
            },
            Field {
                name: Word::from(POI_DIGEST),
                field_type: s::Type::NamedType("String".to_string()),
                value_type: ValueType::String,
                is_derived: false,
            },
        ]
        .into_boxed_slice();
        let name = pool
            .lookup(POI_OBJECT)
            .expect("POI_OBJECT has been interned");
        Self {
            name,
            interfaces: Box::new([]),
            id_type: IdType::String,
            fields,
            shared_interfaces: Box::new([]),
        }
    }

    fn field(&self, name: &str) -> Option<&Field> {
        self.fields.iter().find(|field| field.name == name)
    }
}

#[derive(PartialEq, Debug)]
pub struct InterfaceType {
    pub name: Atom,
    /// For interfaces, the type of the `id` field is the type of the `id`
    /// field of the object types that implement it; validations ensure that
    /// it is the same for all implementers of an interface. If an interface
    /// is not implemented at all, we arbitrarily use `String`
    pub id_type: IdType,
    pub fields: Vec<Field>,
}

impl InterfaceType {
    fn new(
        schema: &Schema,
        pool: &AtomPool,
        interface_type: &s::InterfaceType,
        implementers: &[s::ObjectType],
    ) -> Self {
        let fields = interface_type
            .fields
            .iter()
            .map(|field| Field::new(schema, &field.name, &field.field_type, false))
            .collect();
        let name = pool
            .lookup(&interface_type.name)
            .expect("interface type names have been interned");
        let id_type = implementers
            .first()
            .map(|obj_type| IdType::try_from(obj_type).expect("validation caught any issues here"))
            .unwrap_or(IdType::String);
        Self {
            name,
            id_type,
            fields,
        }
    }
}

#[derive(Debug, PartialEq)]
struct EnumMap(BTreeMap<String, Arc<BTreeSet<String>>>);

impl EnumMap {
    fn new(schema: &Schema) -> Self {
        let map = schema
            .document
            .get_enum_definitions()
            .iter()
            .map(|enum_type| {
                (
                    enum_type.name.clone(),
                    Arc::new(
                        enum_type
                            .values
                            .iter()
                            .map(|value| value.name.clone())
                            .collect::<BTreeSet<_>>(),
                    ),
                )
            })
            .collect();
        EnumMap(map)
    }

    fn names(&self) -> impl Iterator<Item = &str> {
        self.0.keys().map(|name| name.as_str())
    }

    fn contains_key(&self, name: &str) -> bool {
        self.0.contains_key(name)
    }

    fn values(&self, name: &str) -> Option<Arc<BTreeSet<String>>> {
        self.0.get(name).cloned()
    }
}

#[derive(Debug, PartialEq)]
pub struct Inner {
    schema: Schema,
    /// A list of all the object and interface types in the `schema` with
    /// some important information extracted from the schema. The list is
    /// sorted by the name atom (not the string name) of the types
    type_infos: Box<[TypeInfo]>,
    enum_map: EnumMap,
    pool: Arc<AtomPool>,
}

impl CheapClone for InputSchema {
    fn cheap_clone(&self) -> Self {
        InputSchema {
            inner: self.inner.cheap_clone(),
        }
    }
}

impl InputSchema {
    /// A convenience function for creating an `InputSchema` from the string
    /// representation of the subgraph's GraphQL schema `raw` and its
    /// deployment hash `id`. The returned schema is fully validated.
    pub fn parse(raw: &str, id: DeploymentHash) -> Result<Self, Error> {
        let schema = Schema::parse(raw, id.clone())?;
        validations::validate(&schema).map_err(|errors| {
            anyhow!(
                "Validation errors in subgraph `{}`:\n{}",
                id,
                errors
                    .into_iter()
                    .enumerate()
                    .map(|(n, e)| format!("  ({}) - {}", n + 1, e))
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        })?;

        let pool = Arc::new(atom_pool(&schema.document));

        let obj_types = schema
            .document
            .get_object_type_definitions()
            .into_iter()
            .filter(|obj_type| obj_type.name != SCHEMA_TYPE_NAME)
            .map(|obj_type| TypeInfo::for_object(&schema, &pool, obj_type));
        let intf_types = schema
            .document
            .get_interface_type_definitions()
            .into_iter()
            .map(|intf_type| TypeInfo::for_interface(&schema, &pool, intf_type));
        let mut type_infos: Vec<_> = obj_types
            .chain(intf_types)
            .chain(vec![TypeInfo::for_poi(&pool)])
            .collect();
        type_infos.sort_by_key(|ti| ti.name());
        let type_infos = type_infos.into_boxed_slice();

        let enum_map = EnumMap::new(&schema);

        Ok(Self {
            inner: Arc::new(Inner {
                schema,
                type_infos,
                enum_map,
                pool,
            }),
        })
    }

    /// Convenience for tests to construct an `InputSchema`
    ///
    /// # Panics
    ///
    /// If the `document` or `hash` can not be successfully converted
    #[cfg(debug_assertions)]
    #[track_caller]
    pub fn raw(document: &str, hash: &str) -> Self {
        let hash = DeploymentHash::new(hash).unwrap();
        Self::parse(document, hash).unwrap()
    }

    pub fn schema(&self) -> &Schema {
        &self.inner.schema
    }

    /// Generate the `ApiSchema` for use with GraphQL queries for this
    /// `InputSchema`
    pub fn api_schema(&self) -> Result<ApiSchema, anyhow::Error> {
        let mut schema = self.inner.schema.clone();
        schema.document = api_schema(&self.inner.schema)?;
        schema.add_subgraph_id_directives(schema.id.clone());
        ApiSchema::from_api_schema(schema)
    }

    /// Returns the field that has the relationship with the key requested
    /// This works as a reverse search for the Field related to the query
    ///
    /// example:
    ///
    /// type Account @entity {
    ///     wallets: [Wallet!]! @derivedFrom(field: "account")
    /// }
    /// type Wallet {
    ///     account: Account!
    ///     balance: Int!
    /// }
    ///
    /// When asked to load the related entities from "Account" in the field "wallets"
    /// This function will return the type "Wallet" with the field "account"
    pub fn get_field_related(
        &self,
        key: &LoadRelatedRequest,
    ) -> Result<(EntityType, &Field), Error> {
        fn field_err(key: &LoadRelatedRequest, err: &str) -> Error {
            anyhow!(
                "Entity {}[{}]: {err} `{}`",
                key.entity_type,
                key.entity_id,
                key.entity_field,
            )
        }

        let field = self
            .inner
            .schema
            .document
            .get_object_type_definition(key.entity_type.as_str())
            .ok_or_else(|| field_err(key, "unknown entity type"))?
            .field(&key.entity_field)
            .ok_or_else(|| field_err(key, "unknown field"))?;
        if !field.is_derived() {
            return Err(field_err(key, "field is not derived"));
        }

        let derived_from = field.find_directive("derivedFrom").unwrap();
        let entity_type = self.entity_type(field.field_type.get_base_type())?;
        let field_name = derived_from.argument("field").unwrap();

        let field = self
            .find_object_type(entity_type.atom)
            .ok_or_else(|| field_err(key, "unknown entity type"))?
            .field(field_name.as_str().unwrap())
            .ok_or_else(|| field_err(key, "unknown field"))?;

        Ok((entity_type, field))
    }

    fn type_info(&self, atom: Atom) -> Result<&TypeInfo, Error> {
        self.inner
            .type_infos
            .binary_search_by_key(&atom, |ti| ti.name())
            .map(|idx| &self.inner.type_infos[idx])
            .map_err(|_| match self.inner.pool.get(atom) {
                Some(name) => anyhow!(
                    "internal error: entity type `{}` does not exist in {}",
                    name,
                    self.inner.schema.id
                ),
                None => anyhow!(
                    "Invalid atom {atom:?} for type_info lookup in {} (atom is probably from a different pool)",                     self.inner.schema.id

                ),
            })
    }

    pub(in crate::schema) fn id_type(&self, entity_type: Atom) -> Result<store::IdType, Error> {
        let type_info = self.type_info(entity_type)?;

        type_info.kind.id_type().ok_or_else(|| {
            let name = self.inner.pool.get(entity_type).unwrap();
            anyhow!("Entity type `{}` does not have an `id` field", name)
        })
    }

    /// Check if `entity_type` is an immutable object type
    pub(in crate::schema) fn is_immutable(&self, entity_type: Atom) -> bool {
        self.type_info(entity_type)
            .map(|ti| matches!(ti.kind, TypeKind::ImmutableObject(_)))
            .unwrap_or(false)
    }

    /// Return true if `type_name` is the name of an object or interface type
    pub fn is_reference(&self, type_name: &str) -> bool {
        self.inner
            .pool
            .lookup(type_name)
            .and_then(|atom| {
                self.type_info(atom)
                    .ok()
                    .map(|ti| ti.kind.is_object() || ti.kind.is_interface())
            })
            .unwrap_or(false)
    }

    /// Return a list of the interfaces that `entity_type` implements
    pub fn interfaces(&self, entity_type: Atom) -> impl Iterator<Item = &InterfaceType> {
        let obj_type = self.type_info(entity_type).unwrap();
        obj_type.interfaces().map(|intf| {
            let atom = self.inner.pool.lookup(intf).unwrap();
            match self.type_info(atom).unwrap().kind {
                TypeKind::Interface(ref intf_type) => intf_type,
                _ => unreachable!("expected `{intf}` to refer to an interface"),
            }
        })
    }

    /// Return a list of all entity types that implement one of the
    /// interfaces that `entity_type` implements
    pub(in crate::schema) fn share_interfaces(
        &self,
        entity_type: Atom,
    ) -> Result<Vec<EntityType>, Error> {
        let obj_type = match &self.type_info(entity_type)?.kind {
            TypeKind::MutableObject(obj_type) | TypeKind::ImmutableObject(obj_type) => obj_type,
            _ => bail!(
                "expected `{}` to refer to an object type",
                self.inner.pool.get(entity_type).unwrap_or("<unknown>")
            ),
        };
        Ok(obj_type
            .shared_interfaces
            .into_iter()
            .map(|atom| EntityType::new(self.cheap_clone(), *atom))
            .collect())
    }

    pub(in crate::schema) fn find_object_type(&self, entity_type: Atom) -> Option<&ObjectType> {
        self.type_info(entity_type).ok().map(|ti| match &ti.kind {
            TypeKind::MutableObject(obj_type) | TypeKind::ImmutableObject(obj_type) => obj_type,
            TypeKind::Interface(_) => {
                let name = self.inner.pool.get(entity_type).unwrap();
                unreachable!("expected `{}` to refer to an object type", name)
            }
        })
    }

    /// Return a list of the names of all enum types
    pub fn enum_types(&self) -> impl Iterator<Item = &str> {
        self.inner.enum_map.names()
    }

    /// Check if `name` is the name of an enum type
    pub fn is_enum_type(&self, name: &str) -> bool {
        self.inner.enum_map.contains_key(name)
    }

    /// Return a list of the values of the enum type `name`
    pub fn enum_values(&self, name: &str) -> Option<Arc<BTreeSet<String>>> {
        self.inner.enum_map.values(name)
    }

    pub fn entity_types(&self) -> Vec<EntityType> {
        self.inner
            .type_infos
            .iter()
            .filter_map(TypeInfo::object_type)
            .map(|obj_type| EntityType::new(self.cheap_clone(), obj_type.name))
            .collect()
    }

    pub fn entity_fulltext_definitions(
        &self,
        entity: &str,
    ) -> Result<Vec<FulltextDefinition>, anyhow::Error> {
        Self::fulltext_definitions(&self.inner.schema.document, entity)
    }

    fn fulltext_definitions(
        document: &s::Document,
        entity: &str,
    ) -> Result<Vec<FulltextDefinition>, anyhow::Error> {
        Ok(document
            .get_fulltext_directives()?
            .into_iter()
            .filter(|directive| match directive.argument("include") {
                Some(Value::List(includes)) if !includes.is_empty() => {
                    includes.iter().any(|include| match include {
                        Value::Object(include) => match include.get("entity") {
                            Some(Value::String(fulltext_entity)) if fulltext_entity == entity => {
                                true
                            }
                            _ => false,
                        },
                        _ => false,
                    })
                }
                _ => false,
            })
            .map(FulltextDefinition::from)
            .collect())
    }

    pub fn id(&self) -> &DeploymentHash {
        &self.inner.schema.id
    }

    pub fn document_string(&self) -> String {
        self.inner.schema.document.to_string()
    }

    pub fn get_fulltext_directives(&self) -> Result<Vec<&s::Directive>, Error> {
        self.inner.schema.document.get_fulltext_directives()
    }

    pub fn make_entity<I: IntoEntityIterator>(
        &self,
        iter: I,
    ) -> Result<Entity, EntityValidationError> {
        Entity::make(self.inner.pool.clone(), iter)
    }

    pub fn try_make_entity<
        E: std::error::Error + Send + Sync + 'static,
        I: TryIntoEntityIterator<E>,
    >(
        &self,
        iter: I,
    ) -> Result<Entity, Error> {
        Entity::try_make(self.inner.pool.clone(), iter)
    }

    /// Check if `entity_type` is an object type and has a field `field`
    pub(in crate::schema) fn has_field(&self, entity_type: Atom, field: Atom) -> bool {
        self.type_info(entity_type)
            .map(|ti| ti.kind.is_object() && ti.fields.contains(&field))
            .unwrap_or(false)
    }

    pub fn poi_type(&self) -> EntityType {
        // unwrap: we make sure to put POI_OBJECT into the pool
        let atom = self.inner.pool.lookup(POI_OBJECT).unwrap();
        EntityType::new(self.cheap_clone(), atom)
    }

    pub fn poi_digest(&self) -> Word {
        Word::from(POI_DIGEST)
    }

    // A helper for the `EntityType` constructor
    pub(in crate::schema) fn pool(&self) -> &Arc<AtomPool> {
        &self.inner.pool
    }

    /// Return the entity type for `named`. If the entity type does not
    /// exist, return an error. Generally, an error should only be possible
    /// if `named` is based on user input. If `named` is an internal object,
    /// like a `ObjectType`, it is safe to unwrap the result
    pub fn entity_type<N: AsEntityTypeName>(&self, named: N) -> Result<EntityType, Error> {
        let name = named.name();
        self.inner
            .pool
            .lookup(name)
            .and_then(|atom| self.type_info(atom).ok())
            .map(|ti| EntityType::new(self.cheap_clone(), ti.name()))
            .ok_or_else(|| {
                anyhow!(
                    "internal error: entity type `{}` does not exist in {}",
                    name,
                    self.inner.schema.id
                )
            })
    }
    pub fn has_field_with_name(&self, entity_type: &EntityType, field: &str) -> bool {
        let field = self.inner.pool.lookup(field);

        match field {
            Some(field) => self.has_field(entity_type.atom, field),
            None => false,
        }
    }
}

/// Create a new pool that contains the names of all the types defined
/// in the document and the names of all their fields
fn atom_pool(document: &s::Document) -> AtomPool {
    let mut pool = AtomPool::new();

    pool.intern(&*ID);
    pool.intern(POI_OBJECT); // Name of PoI entity type
    pool.intern(POI_DIGEST); // Attribute of PoI object

    for definition in &document.definitions {
        match definition {
            s::Definition::TypeDefinition(typedef) => match typedef {
                s::TypeDefinition::Object(t) => {
                    pool.intern(&t.name);
                    for field in &t.fields {
                        pool.intern(&field.name);
                    }
                }
                s::TypeDefinition::Enum(t) => {
                    pool.intern(&t.name);
                }
                s::TypeDefinition::Interface(t) => {
                    pool.intern(&t.name);
                    for field in &t.fields {
                        pool.intern(&field.name);
                    }
                }
                s::TypeDefinition::InputObject(input_object) => {
                    pool.intern(&input_object.name);
                    for field in &input_object.fields {
                        pool.intern(&field.name);
                    }
                }
                s::TypeDefinition::Scalar(scalar_type) => {
                    pool.intern(&scalar_type.name);
                }
                s::TypeDefinition::Union(union_type) => {
                    pool.intern(&union_type.name);
                    for typ in &union_type.types {
                        pool.intern(typ);
                    }
                }
            },
            s::Definition::SchemaDefinition(_)
            | s::Definition::TypeExtension(_)
            | s::Definition::DirectiveDefinition(_) => { /* ignore, these only happen for introspection schemas */
            }
        }
    }

    for object_type in document.get_object_type_definitions() {
        for defn in InputSchema::fulltext_definitions(&document, &object_type.name).unwrap() {
            pool.intern(defn.name.as_str());
        }
    }

    pool
}

/// Validations for an `InputSchema`.
mod validations {
    use std::{collections::HashSet, str::FromStr};

    use inflector::Inflector;
    use itertools::Itertools;

    use crate::{
        data::{
            graphql::{
                ext::DirectiveFinder, DirectiveExt, DocumentExt, ObjectTypeExt, TypeExt, ValueExt,
            },
            store::{IdType, ValueType, ID},
        },
        prelude::s,
        schema::{
            FulltextAlgorithm, FulltextLanguage, Schema as BaseSchema, SchemaValidationError,
            Strings, SCHEMA_TYPE_NAME,
        },
    };

    /// Helper struct for validations
    struct Schema<'a> {
        schema: &'a BaseSchema,
        subgraph_schema_type: Option<&'a s::ObjectType>,
        // All entity types, excluding the subgraph schema type
        entity_types: Vec<&'a s::ObjectType>,
    }

    pub(super) fn validate(schema: &BaseSchema) -> Result<(), Vec<SchemaValidationError>> {
        let schema = Schema::new(schema);

        let mut errors: Vec<SchemaValidationError> = [
            schema.validate_no_extra_types(),
            schema.validate_derived_from(),
            schema.validate_schema_type_has_no_fields(),
            schema.validate_directives_on_schema_type(),
            schema.validate_reserved_types_usage(),
            schema.validate_interface_id_type(),
        ]
        .into_iter()
        .filter(Result::is_err)
        // Safe unwrap due to the filter above
        .map(Result::unwrap_err)
        .collect();

        errors.append(&mut schema.validate_entity_type_ids());
        errors.append(&mut schema.validate_fields());
        errors.append(&mut schema.validate_fulltext_directives());

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    impl<'a> Schema<'a> {
        fn new(schema: &'a BaseSchema) -> Self {
            let subgraph_schema_type = schema.subgraph_schema_object_type();
            let mut entity_types = schema.document.get_object_type_definitions();
            entity_types.retain(|obj_type| obj_type.name != SCHEMA_TYPE_NAME);

            Schema {
                schema,
                subgraph_schema_type,
                entity_types,
            }
        }

        fn validate_schema_type_has_no_fields(&self) -> Result<(), SchemaValidationError> {
            match self.subgraph_schema_type.and_then(|subgraph_schema_type| {
                if !subgraph_schema_type.fields.is_empty() {
                    Some(SchemaValidationError::SchemaTypeWithFields)
                } else {
                    None
                }
            }) {
                Some(err) => Err(err),
                None => Ok(()),
            }
        }

        fn validate_directives_on_schema_type(&self) -> Result<(), SchemaValidationError> {
            match self.subgraph_schema_type.and_then(|subgraph_schema_type| {
                if subgraph_schema_type
                    .directives
                    .iter()
                    .filter(|directive| !directive.name.eq("fulltext"))
                    .next()
                    .is_some()
                {
                    Some(SchemaValidationError::InvalidSchemaTypeDirectives)
                } else {
                    None
                }
            }) {
                Some(err) => Err(err),
                None => Ok(()),
            }
        }

        fn validate_fulltext_directives(&self) -> Vec<SchemaValidationError> {
            self.subgraph_schema_type
                .map_or(vec![], |subgraph_schema_type| {
                    subgraph_schema_type
                        .directives
                        .iter()
                        .filter(|directives| directives.name.eq("fulltext"))
                        .fold(vec![], |mut errors, fulltext| {
                            errors.extend(self.validate_fulltext_directive_name(fulltext));
                            errors.extend(self.validate_fulltext_directive_language(fulltext));
                            errors.extend(self.validate_fulltext_directive_algorithm(fulltext));
                            errors.extend(self.validate_fulltext_directive_includes(fulltext));
                            errors
                        })
                })
        }

        fn validate_fulltext_directive_name(
            &self,
            fulltext: &s::Directive,
        ) -> Vec<SchemaValidationError> {
            let name = match fulltext.argument("name") {
                Some(s::Value::String(name)) => name,
                _ => return vec![SchemaValidationError::FulltextNameUndefined],
            };

            // Validate that the fulltext field doesn't collide with any top-level Query fields
            // generated for entity types. The field name conversions should always align with those used
            // to create the field names in `graphql::schema::api::query_fields_for_type()`.
            if self.entity_types.iter().any(|typ| {
                typ.fields.iter().any(|field| {
                    name == &field.name.as_str().to_camel_case()
                        || name == &field.name.to_plural().to_camel_case()
                        || field.name.eq(name)
                })
            }) {
                return vec![SchemaValidationError::FulltextNameCollision(
                    name.to_string(),
                )];
            }

            // Validate that each fulltext directive has a distinct name
            if self
                .subgraph_schema_type
                .unwrap()
                .directives
                .iter()
                .filter(|directive| directive.name.eq("fulltext"))
                .filter_map(|fulltext| {
                    // Collect all @fulltext directives with the same name
                    match fulltext.argument("name") {
                        Some(s::Value::String(n)) if name.eq(n) => Some(n.as_str()),
                        _ => None,
                    }
                })
                .count()
                > 1
            {
                vec![SchemaValidationError::FulltextNameConflict(
                    name.to_string(),
                )]
            } else {
                vec![]
            }
        }

        fn validate_fulltext_directive_language(
            &self,
            fulltext: &s::Directive,
        ) -> Vec<SchemaValidationError> {
            let language = match fulltext.argument("language") {
                Some(s::Value::Enum(language)) => language,
                _ => return vec![SchemaValidationError::FulltextLanguageUndefined],
            };
            match FulltextLanguage::try_from(language.as_str()) {
                Ok(_) => vec![],
                Err(_) => vec![SchemaValidationError::FulltextLanguageInvalid(
                    language.to_string(),
                )],
            }
        }

        fn validate_fulltext_directive_algorithm(
            &self,
            fulltext: &s::Directive,
        ) -> Vec<SchemaValidationError> {
            let algorithm = match fulltext.argument("algorithm") {
                Some(s::Value::Enum(algorithm)) => algorithm,
                _ => return vec![SchemaValidationError::FulltextAlgorithmUndefined],
            };
            match FulltextAlgorithm::try_from(algorithm.as_str()) {
                Ok(_) => vec![],
                Err(_) => vec![SchemaValidationError::FulltextAlgorithmInvalid(
                    algorithm.to_string(),
                )],
            }
        }

        fn validate_fulltext_directive_includes(
            &self,
            fulltext: &s::Directive,
        ) -> Vec<SchemaValidationError> {
            // Validate that each entity in fulltext.include exists
            let includes = match fulltext.argument("include") {
                Some(s::Value::List(includes)) if !includes.is_empty() => includes,
                _ => return vec![SchemaValidationError::FulltextIncludeUndefined],
            };

            for include in includes {
                match include.as_object() {
                    None => return vec![SchemaValidationError::FulltextIncludeObjectMissing],
                    Some(include_entity) => {
                        let (entity, fields) =
                        match (include_entity.get("entity"), include_entity.get("fields")) {
                            (Some(s::Value::String(entity)), Some(s::Value::List(fields))) => {
                                (entity, fields)
                            }
                            _ => return vec![SchemaValidationError::FulltextIncludeEntityMissingOrIncorrectAttributes],
                        };

                        // Validate the included entity type is one of the local types
                        let entity_type = match self
                            .entity_types
                            .iter()
                            .cloned()
                            .find(|typ| typ.name[..].eq(entity))
                        {
                            None => {
                                return vec![SchemaValidationError::FulltextIncludedEntityNotFound]
                            }
                            Some(t) => t,
                        };

                        for field_value in fields {
                            let field_name = match field_value {
                            s::Value::Object(field_map) => match field_map.get("name") {
                                Some(s::Value::String(name)) => name,
                                _ => return vec![SchemaValidationError::FulltextIncludedFieldMissingRequiredProperty],
                            },
                            _ => return vec![SchemaValidationError::FulltextIncludeEntityMissingOrIncorrectAttributes],
                        };

                            // Validate the included field is a String field on the local entity types specified
                            if !&entity_type
                            .fields
                            .iter()
                            .any(|field| {
                                let base_type: &str = field.field_type.get_base_type();
                                matches!(ValueType::from_str(base_type), Ok(ValueType::String) if field.name.eq(field_name))
                            })
                        {
                            return vec![SchemaValidationError::FulltextIncludedFieldInvalid(
                                field_name.clone(),
                            )];
                        };
                        }
                    }
                }
            }
            // Fulltext include validations all passed, so we return an empty vector
            vec![]
        }

        fn validate_fields(&self) -> Vec<SchemaValidationError> {
            let local_types = self.schema.document.get_object_and_interface_type_fields();
            let local_enums = self
                .schema
                .document
                .get_enum_definitions()
                .iter()
                .map(|enu| enu.name.clone())
                .collect::<Vec<String>>();
            local_types
                .iter()
                .fold(vec![], |errors, (type_name, fields)| {
                    fields.iter().fold(errors, |mut errors, field| {
                        let base = field.field_type.get_base_type();
                        if ValueType::is_scalar(base) {
                            return errors;
                        }
                        if local_types.contains_key(base) {
                            return errors;
                        }
                        if local_enums.iter().any(|enu| enu.eq(base)) {
                            return errors;
                        }
                        errors.push(SchemaValidationError::FieldTypeUnknown(
                            type_name.to_string(),
                            field.name.to_string(),
                            base.to_string(),
                        ));
                        errors
                    })
                })
        }

        /// 1. All object types besides `_Schema_` must have an id field
        /// 2. The id field must be recognized by IdType
        fn validate_entity_type_ids(&self) -> Vec<SchemaValidationError> {
            self.entity_types
                .iter()
                .fold(vec![], |mut errors, object_type| {
                    match object_type.field(&*ID) {
                        None => errors.push(SchemaValidationError::IdFieldMissing(
                            object_type.name.clone(),
                        )),
                        Some(_) => {
                            if let Err(e) = IdType::try_from(*object_type) {
                                errors.push(SchemaValidationError::IllegalIdType(e.to_string()));
                            }
                        }
                    }
                    errors
                })
        }

        /// Checks if the schema is using types that are reserved
        /// by `graph-node`
        fn validate_reserved_types_usage(&self) -> Result<(), SchemaValidationError> {
            let document = &self.schema.document;
            let object_types: Vec<_> = document
                .get_object_type_definitions()
                .into_iter()
                .map(|obj_type| &obj_type.name)
                .collect();

            let interface_types: Vec<_> = document
                .get_interface_type_definitions()
                .into_iter()
                .map(|iface_type| &iface_type.name)
                .collect();

            // TYPE_NAME_filter types for all object and interface types
            let mut filter_types: Vec<String> = object_types
                .iter()
                .chain(interface_types.iter())
                .map(|type_name| format!("{}_filter", type_name))
                .collect();

            // TYPE_NAME_orderBy types for all object and interface types
            let mut order_by_types: Vec<_> = object_types
                .iter()
                .chain(interface_types.iter())
                .map(|type_name| format!("{}_orderBy", type_name))
                .collect();

            let mut reserved_types: Vec<String> = vec![
                // The built-in scalar types
                "Boolean".into(),
                "ID".into(),
                "Int".into(),
                "BigDecimal".into(),
                "String".into(),
                "Bytes".into(),
                "BigInt".into(),
                // Reserved Query and Subscription types
                "Query".into(),
                "Subscription".into(),
            ];

            reserved_types.append(&mut filter_types);
            reserved_types.append(&mut order_by_types);

            // `reserved_types` will now only contain
            // the reserved types that the given schema *is* using.
            //
            // That is, if the schema is compliant and not using any reserved
            // types, then it'll become an empty vector
            reserved_types.retain(|reserved_type| document.get_named_type(reserved_type).is_some());

            if reserved_types.is_empty() {
                Ok(())
            } else {
                Err(SchemaValidationError::UsageOfReservedTypes(Strings(
                    reserved_types,
                )))
            }
        }

        fn validate_no_extra_types(&self) -> Result<(), SchemaValidationError> {
            let types_without_entity_directive = self
                .schema
                .document
                .get_object_type_definitions()
                .iter()
                .filter(|t| t.find_directive("entity").is_none() && !t.name.eq(SCHEMA_TYPE_NAME))
                .map(|t| t.name.clone())
                .collect::<Vec<_>>();
            if types_without_entity_directive.is_empty() {
                Ok(())
            } else {
                Err(SchemaValidationError::EntityDirectivesMissing(Strings(
                    types_without_entity_directive,
                )))
            }
        }

        fn validate_derived_from(&self) -> Result<(), SchemaValidationError> {
            // Helper to construct a DerivedFromInvalid
            fn invalid(
                object_type: &s::ObjectType,
                field_name: &str,
                reason: &str,
            ) -> SchemaValidationError {
                SchemaValidationError::InvalidDerivedFrom(
                    object_type.name.clone(),
                    field_name.to_owned(),
                    reason.to_owned(),
                )
            }

            let object_and_interface_type_fields =
                self.schema.document.get_object_and_interface_type_fields();

            // Iterate over all derived fields in all entity types; include the
            // interface types that the entity with the `@derivedFrom` implements
            // and the `field` argument of @derivedFrom directive
            for (object_type, interface_types, field, target_field) in self
                .entity_types
                .iter()
                .flat_map(|object_type| {
                    object_type
                        .fields
                        .iter()
                        .map(move |field| (object_type, field))
                })
                .filter_map(|(object_type, field)| {
                    field.find_directive("derivedFrom").map(|directive| {
                        (
                            object_type,
                            object_type
                                .implements_interfaces
                                .iter()
                                .filter(|iface| {
                                    // Any interface that has `field` can be used
                                    // as the type of the field
                                    self.schema
                                        .document
                                        .find_interface(iface)
                                        .map(|iface| {
                                            iface
                                                .fields
                                                .iter()
                                                .any(|ifield| ifield.name.eq(&field.name))
                                        })
                                        .unwrap_or(false)
                                })
                                .collect::<Vec<_>>(),
                            field,
                            directive.argument("field"),
                        )
                    })
                })
            {
                // Turn `target_field` into the string name of the field
                let target_field = target_field.ok_or_else(|| {
                    invalid(
                        object_type,
                        &field.name,
                        "the @derivedFrom directive must have a `field` argument",
                    )
                })?;
                let target_field = match target_field {
                    s::Value::String(s) => s,
                    _ => {
                        return Err(invalid(
                            object_type,
                            &field.name,
                            "the @derivedFrom `field` argument must be a string",
                        ))
                    }
                };

                // Check that the type we are deriving from exists
                let target_type_name = field.field_type.get_base_type();
                let target_fields = object_and_interface_type_fields
                    .get(target_type_name)
                    .ok_or_else(|| {
                        invalid(
                            object_type,
                            &field.name,
                            "type must be an existing entity or interface",
                        )
                    })?;

                // Check that the type we are deriving from has a field with the
                // right name and type
                let target_field = target_fields
                    .iter()
                    .find(|field| field.name.eq(target_field))
                    .ok_or_else(|| {
                        let msg = format!(
                            "field `{}` does not exist on type `{}`",
                            target_field, target_type_name
                        );
                        invalid(object_type, &field.name, &msg)
                    })?;

                // The field we are deriving from has to point back to us; as an
                // exception, we allow deriving from the `id` of another type.
                // For that, we will wind up comparing the `id`s of the two types
                // when we query, and just assume that that's ok.
                let target_field_type = target_field.field_type.get_base_type();
                if target_field_type != object_type.name
                    && &target_field.name != ID.as_str()
                    && !interface_types
                        .iter()
                        .any(|iface| target_field_type.eq(iface.as_str()))
                {
                    fn type_signatures(name: &str) -> Vec<String> {
                        vec![
                            format!("{}", name),
                            format!("{}!", name),
                            format!("[{}!]", name),
                            format!("[{}!]!", name),
                        ]
                    }

                    let mut valid_types = type_signatures(&object_type.name);
                    valid_types.extend(
                        interface_types
                            .iter()
                            .flat_map(|iface| type_signatures(iface)),
                    );
                    let valid_types = valid_types.join(", ");

                    let msg = format!(
                    "field `{tf}` on type `{tt}` must have one of the following types: {valid_types}",
                    tf = target_field.name,
                    tt = target_type_name,
                    valid_types = valid_types,
                );
                    return Err(invalid(object_type, &field.name, &msg));
                }
            }
            Ok(())
        }

        fn validate_interface_id_type(&self) -> Result<(), SchemaValidationError> {
            for (intf, obj_types) in &self.schema.types_for_interface {
                let id_types: HashSet<&str> = HashSet::from_iter(
                    obj_types
                        .iter()
                        .filter_map(|obj_type| obj_type.field(&*ID))
                        .map(|f| f.field_type.get_base_type())
                        .map(|name| if name == "ID" { "String" } else { name }),
                );
                if id_types.len() > 1 {
                    return Err(SchemaValidationError::InterfaceImplementorsMixId(
                        intf.to_string(),
                        id_types.iter().join(", "),
                    ));
                }
            }
            Ok(())
        }
    }

    #[cfg(test)]
    mod tests {
        use crate::prelude::DeploymentHash;

        use super::*;

        fn parse(schema: &str) -> BaseSchema {
            let hash = DeploymentHash::new("test").unwrap();
            BaseSchema::parse(schema, hash).unwrap()
        }

        #[test]
        fn object_types_have_id() {
            const NO_ID: &str = "type User @entity { name: String! }";
            const ID_BIGINT: &str = "type User @entity { id: BigInt! }";
            const INTF_NO_ID: &str = "interface Person { name: String! }";
            const ROOT_SCHEMA: &str = "type _Schema_";

            let res = validate(&parse(NO_ID));
            assert_eq!(
                res,
                Err(vec![SchemaValidationError::IdFieldMissing(
                    "User".to_string()
                )])
            );

            let res = validate(&parse(ID_BIGINT));
            let errs = res.unwrap_err();
            assert_eq!(1, errs.len());
            assert!(matches!(errs[0], SchemaValidationError::IllegalIdType(_)));

            let res = validate(&parse(INTF_NO_ID));
            assert_eq!(Ok(()), res);

            let res = validate(&parse(ROOT_SCHEMA));
            assert_eq!(Ok(()), res);
        }

        #[test]
        fn interface_implementations_id_type() {
            fn check_schema(bar_id: &str, baz_id: &str, ok: bool) {
                let schema = format!(
                    "interface Foo {{ x: Int }}
             type Bar implements Foo @entity {{
                id: {bar_id}!
                x: Int
             }}

             type Baz implements Foo @entity {{
                id: {baz_id}!
                x: Int
            }}"
                );
                let schema =
                    BaseSchema::parse(&schema, DeploymentHash::new("dummy").unwrap()).unwrap();
                let res = validate(&schema);
                if ok {
                    assert!(matches!(res, Ok(_)));
                } else {
                    assert!(matches!(res, Err(_)));
                    assert!(matches!(
                        res.unwrap_err()[0],
                        SchemaValidationError::InterfaceImplementorsMixId(_, _)
                    ));
                }
            }
            check_schema("ID", "ID", true);
            check_schema("ID", "String", true);
            check_schema("ID", "Bytes", false);
            check_schema("Bytes", "String", false);
        }

        #[test]
        fn test_derived_from_validation() {
            const OTHER_TYPES: &str = "
type B @entity { id: ID! }
type C @entity { id: ID! }
type D @entity { id: ID! }
type E @entity { id: ID! }
type F @entity { id: ID! }
type G @entity { id: ID! a: BigInt }
type H @entity { id: ID! a: A! }
# This sets up a situation where we need to allow `Transaction.from` to
# point to an interface because of `Account.txn`
type Transaction @entity { from: Address! }
interface Address { txn: Transaction! @derivedFrom(field: \"from\") }
type Account implements Address @entity { id: ID!, txn: Transaction! @derivedFrom(field: \"from\") }";

            fn validate(field: &str, errmsg: &str) {
                let raw = format!("type A @entity {{ id: ID!\n {} }}\n{}", field, OTHER_TYPES);

                let document = graphql_parser::parse_schema(&raw)
                    .expect("Failed to parse raw schema")
                    .into_static();
                let schema = BaseSchema::new(DeploymentHash::new("id").unwrap(), document).unwrap();
                let schema = Schema::new(&schema);
                match schema.validate_derived_from() {
                    Err(ref e) => match e {
                        SchemaValidationError::InvalidDerivedFrom(_, _, msg) => {
                            assert_eq!(errmsg, msg)
                        }
                        _ => panic!("expected variant SchemaValidationError::DerivedFromInvalid"),
                    },
                    Ok(_) => {
                        if errmsg != "ok" {
                            panic!("expected validation for `{}` to fail", field)
                        }
                    }
                }
            }

            validate(
                "b: B @derivedFrom(field: \"a\")",
                "field `a` does not exist on type `B`",
            );
            validate(
                "c: [C!]! @derivedFrom(field: \"a\")",
                "field `a` does not exist on type `C`",
            );
            validate(
                "d: D @derivedFrom",
                "the @derivedFrom directive must have a `field` argument",
            );
            validate(
                "e: E @derivedFrom(attr: \"a\")",
                "the @derivedFrom directive must have a `field` argument",
            );
            validate(
                "f: F @derivedFrom(field: 123)",
                "the @derivedFrom `field` argument must be a string",
            );
            validate(
                "g: G @derivedFrom(field: \"a\")",
                "field `a` on type `G` must have one of the following types: A, A!, [A!], [A!]!",
            );
            validate("h: H @derivedFrom(field: \"a\")", "ok");
            validate(
                "i: NotAType @derivedFrom(field: \"a\")",
                "type must be an existing entity or interface",
            );
            validate("j: B @derivedFrom(field: \"id\")", "ok");
        }

        #[test]
        fn test_reserved_type_with_fields() {
            const ROOT_SCHEMA: &str = "
type _Schema_ { id: ID! }";

            let document =
                graphql_parser::parse_schema(ROOT_SCHEMA).expect("Failed to parse root schema");
            let schema = BaseSchema::new(DeploymentHash::new("id").unwrap(), document).unwrap();
            let schema = Schema::new(&schema);
            assert_eq!(
                schema.validate_schema_type_has_no_fields().expect_err(
                    "Expected validation to fail due to fields defined on the reserved type"
                ),
                SchemaValidationError::SchemaTypeWithFields
            )
        }

        #[test]
        fn test_reserved_type_directives() {
            const ROOT_SCHEMA: &str = "
type _Schema_ @illegal";

            let document =
                graphql_parser::parse_schema(ROOT_SCHEMA).expect("Failed to parse root schema");
            let schema = BaseSchema::new(DeploymentHash::new("id").unwrap(), document).unwrap();
            let schema = Schema::new(&schema);
            assert_eq!(
                schema.validate_directives_on_schema_type().expect_err(
                    "Expected validation to fail due to extra imports defined on the reserved type"
                ),
                SchemaValidationError::InvalidSchemaTypeDirectives
            )
        }

        #[test]
        fn test_enums_pass_field_validation() {
            const ROOT_SCHEMA: &str = r#"
enum Color {
  RED
  GREEN
}

type A @entity {
  id: ID!
  color: Color
}"#;

            let document =
                graphql_parser::parse_schema(ROOT_SCHEMA).expect("Failed to parse root schema");
            let schema = BaseSchema::new(DeploymentHash::new("id").unwrap(), document).unwrap();
            let schema = Schema::new(&schema);
            assert_eq!(schema.validate_fields().len(), 0);
        }

        #[test]
        fn test_reserved_types_validation() {
            let reserved_types = [
                // Built-in scalars
                "Boolean",
                "ID",
                "Int",
                "BigDecimal",
                "String",
                "Bytes",
                "BigInt",
                // Reserved keywords
                "Query",
                "Subscription",
            ];

            let dummy_hash = DeploymentHash::new("dummy").unwrap();

            for reserved_type in reserved_types {
                let schema = format!(
                    "type {} @entity {{ id: String! _: Boolean }}\n",
                    reserved_type
                );

                let schema = BaseSchema::parse(&schema, dummy_hash.clone()).unwrap();

                let errors = validate(&schema).unwrap_err();
                for error in errors {
                    assert!(matches!(
                        error,
                        SchemaValidationError::UsageOfReservedTypes(_)
                    ))
                }
            }
        }

        #[test]
        fn test_reserved_filter_and_group_by_types_validation() {
            const SCHEMA: &str = r#"
    type Gravatar @entity {
        id: String!
        _: Boolean
      }
    type Gravatar_filter @entity {
        id: String!
        _: Boolean
    }
    type Gravatar_orderBy @entity {
        id: String!
        _: Boolean
    }
    "#;

            let dummy_hash = DeploymentHash::new("dummy").unwrap();

            let schema = BaseSchema::parse(SCHEMA, dummy_hash).unwrap();

            let errors = validate(&schema).unwrap_err();

            // The only problem in the schema is the usage of reserved types
            assert_eq!(errors.len(), 1);

            assert!(matches!(
                &errors[0],
                SchemaValidationError::UsageOfReservedTypes(Strings(_))
            ));

            // We know this will match due to the assertion above
            match &errors[0] {
                SchemaValidationError::UsageOfReservedTypes(Strings(reserved_types)) => {
                    let expected_types: Vec<String> =
                        vec!["Gravatar_filter".into(), "Gravatar_orderBy".into()];
                    assert_eq!(reserved_types, &expected_types);
                }
                _ => unreachable!(),
            }
        }

        #[test]
        fn test_fulltext_directive_validation() {
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
}"#;

            let document = graphql_parser::parse_schema(SCHEMA).expect("Failed to parse schema");
            let schema = BaseSchema::new(DeploymentHash::new("id1").unwrap(), document).unwrap();
            let schema = Schema::new(&schema);
            assert_eq!(schema.validate_fulltext_directives(), vec![]);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        data::store::ID,
        prelude::DeploymentHash,
        schema::input_schema::{POI_DIGEST, POI_OBJECT},
    };

    use super::InputSchema;

    const SCHEMA: &str = r#"
      type Thing @entity {
        id: ID!
        name: String!
      }
    "#;

    #[test]
    fn entity_type() {
        let id = DeploymentHash::new("test").unwrap();
        let schema = InputSchema::parse(SCHEMA, id).unwrap();

        assert_eq!("Thing", schema.entity_type("Thing").unwrap().as_str());

        let poi = schema.entity_type(POI_OBJECT).unwrap();
        assert_eq!(POI_OBJECT, poi.as_str());
        assert!(poi.has_field(schema.pool().lookup(&ID).unwrap()));
        assert!(poi.has_field(schema.pool().lookup(POI_DIGEST).unwrap()));
        assert!(poi.object_type().is_some());

        assert!(schema.entity_type("NonExistent").is_err());
    }

    #[test]
    fn share_interfaces() {
        const SCHEMA: &str = r#"
    interface Animal {
        name: String!
    }

    type Dog implements Animal @entity {
        id: ID!
        name: String!
    }

    type Cat implements Animal @entity {
        id: ID!
        name: String!
    }

    type Person @entity {
        id: ID!
        name: String!
    }
        "#;

        let id = DeploymentHash::new("test").unwrap();
        let schema = InputSchema::parse(SCHEMA, id).unwrap();

        let dog = schema.entity_type("Dog").unwrap();
        let cat = schema.entity_type("Cat").unwrap();
        let person = schema.entity_type("Person").unwrap();
        assert_eq!(vec![cat.clone()], dog.share_interfaces().unwrap());
        assert_eq!(vec![dog], cat.share_interfaces().unwrap());
        assert!(person.share_interfaces().unwrap().is_empty());
    }
}
