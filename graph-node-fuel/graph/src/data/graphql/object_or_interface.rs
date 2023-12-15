use crate::prelude::s;
use crate::schema::{EntityType, Schema};
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};
use std::mem;

use super::ObjectTypeExt;

#[derive(Copy, Clone, Debug)]
pub enum ObjectOrInterface<'a> {
    Object(&'a s::ObjectType),
    Interface(&'a s::InterfaceType),
}

impl<'a> PartialEq for ObjectOrInterface<'a> {
    fn eq(&self, other: &Self) -> bool {
        use ObjectOrInterface::*;
        match (self, other) {
            (Object(a), Object(b)) => a.name == b.name,
            (Interface(a), Interface(b)) => a.name == b.name,
            (Interface(_), Object(_)) | (Object(_), Interface(_)) => false,
        }
    }
}

impl<'a> Eq for ObjectOrInterface<'a> {}

impl<'a> Hash for ObjectOrInterface<'a> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        mem::discriminant(self).hash(state);
        self.name().hash(state)
    }
}

impl<'a> PartialOrd for ObjectOrInterface<'a> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<'a> Ord for ObjectOrInterface<'a> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        use ObjectOrInterface::*;
        match (self, other) {
            (Object(a), Object(b)) => a.name.cmp(&b.name),
            (Interface(a), Interface(b)) => a.name.cmp(&b.name),
            (Interface(_), Object(_)) => Ordering::Less,
            (Object(_), Interface(_)) => Ordering::Greater,
        }
    }
}

impl<'a> From<&'a s::ObjectType> for ObjectOrInterface<'a> {
    fn from(object: &'a s::ObjectType) -> Self {
        ObjectOrInterface::Object(object)
    }
}

impl<'a> From<&'a s::InterfaceType> for ObjectOrInterface<'a> {
    fn from(interface: &'a s::InterfaceType) -> Self {
        ObjectOrInterface::Interface(interface)
    }
}

impl<'a> ObjectOrInterface<'a> {
    pub fn is_object(self) -> bool {
        match self {
            ObjectOrInterface::Object(_) => true,
            ObjectOrInterface::Interface(_) => false,
        }
    }

    pub fn is_interface(self) -> bool {
        match self {
            ObjectOrInterface::Object(_) => false,
            ObjectOrInterface::Interface(_) => true,
        }
    }

    pub fn name(self) -> &'a str {
        match self {
            ObjectOrInterface::Object(object) => &object.name,
            ObjectOrInterface::Interface(interface) => &interface.name,
        }
    }

    pub fn directives(self) -> &'a Vec<s::Directive> {
        match self {
            ObjectOrInterface::Object(object) => &object.directives,
            ObjectOrInterface::Interface(interface) => &interface.directives,
        }
    }

    pub fn fields(self) -> &'a Vec<s::Field> {
        match self {
            ObjectOrInterface::Object(object) => &object.fields,
            ObjectOrInterface::Interface(interface) => &interface.fields,
        }
    }

    pub fn field(&self, name: &str) -> Option<&s::Field> {
        self.fields().iter().find(|field| &field.name == name)
    }

    pub fn object_types(self, schema: &'a Schema) -> Option<Vec<&'a s::ObjectType>> {
        match self {
            ObjectOrInterface::Object(object) => Some(vec![object]),
            ObjectOrInterface::Interface(interface) => schema
                .types_for_interface()
                .get(interface.name.as_str())
                .map(|object_types| object_types.iter().collect()),
        }
    }

    /// `typename` is the name of an object type. Matches if `self` is an object and has the same
    /// name, or if self is an interface implemented by `typename`.
    pub fn matches(
        self,
        typename: &str,
        types_for_interface: &BTreeMap<EntityType, Vec<s::ObjectType>>,
    ) -> bool {
        match self {
            ObjectOrInterface::Object(o) => o.name == typename,
            ObjectOrInterface::Interface(i) => types_for_interface[i.name.as_str()]
                .iter()
                .any(|o| o.name == typename),
        }
    }

    pub fn is_meta(&self) -> bool {
        match self {
            ObjectOrInterface::Object(o) => o.is_meta(),
            ObjectOrInterface::Interface(i) => i.is_meta(),
        }
    }
}
