//! Types and helpers to deal with entity IDs which support a subset of the
//! types that more general values support
use anyhow::{anyhow, Error};
use stable_hash::{StableHash, StableHasher};
use std::convert::TryFrom;
use std::fmt;

use crate::{
    anyhow, bail,
    components::store::BlockNumber,
    data::graphql::{ObjectTypeExt, TypeExt},
    prelude::s,
};

use crate::{
    components::store::StoreError,
    constraint_violation,
    data::value::Word,
    prelude::{CacheWeight, QueryExecutionError},
    runtime::gas::{Gas, GasSizeOf},
    schema::EntityType,
};

use super::{scalar, Value, ValueType, ID};

/// The types that can be used for the `id` of an entity
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum IdType {
    String,
    Bytes,
    Int8,
}

impl IdType {
    /// Parse the given string into an ID of this type
    pub fn parse(&self, s: Word) -> Result<Id, Error> {
        match self {
            IdType::String => Ok(Id::String(s)),
            IdType::Bytes => Ok(Id::Bytes(s.parse()?)),
            IdType::Int8 => Ok(Id::Int8(s.parse()?)),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            IdType::String => "String",
            IdType::Bytes => "Bytes",
            IdType::Int8 => "Int8",
        }
    }

    /// Generate an entity id from the block number and a sequence number.
    ///
    /// * Bytes: `[block:4, seq:4]`
    /// * Int8: `[block:4, seq:4]`
    /// * String: Always an error; users should use `Bytes` or `Int8`
    ///   instead
    pub fn generate_id(&self, block: BlockNumber, seq: u32) -> anyhow::Result<Id> {
        match self {
            IdType::String => bail!("String does not support generating ids"),
            IdType::Bytes => {
                let mut bytes = [0u8; 8];
                bytes[0..4].copy_from_slice(&block.to_be_bytes());
                bytes[4..8].copy_from_slice(&seq.to_be_bytes());
                let bytes = scalar::Bytes::from(bytes);
                Ok(Id::Bytes(bytes))
            }
            IdType::Int8 => {
                let mut bytes = [0u8; 8];
                bytes[0..4].copy_from_slice(&seq.to_le_bytes());
                bytes[4..8].copy_from_slice(&block.to_le_bytes());
                Ok(Id::Int8(i64::from_le_bytes(bytes)))
            }
        }
    }
}

impl<'a> TryFrom<&s::ObjectType> for IdType {
    type Error = Error;

    fn try_from(obj_type: &s::ObjectType) -> Result<Self, Self::Error> {
        let base_type = obj_type
            .field(&*ID)
            .ok_or_else(|| anyhow!("Type {} does not have an `id` field", obj_type.name))?
            .field_type
            .get_base_type();

        match base_type {
            "ID" | "String" => Ok(IdType::String),
            "Bytes" => Ok(IdType::Bytes),
            "Int8" => Ok(IdType::Int8),
            s => Err(anyhow!(
                "Entity type {} uses illegal type {} for id column",
                obj_type.name,
                s
            )),
        }
    }
}

impl TryFrom<&s::Type> for IdType {
    type Error = StoreError;

    fn try_from(field_type: &s::Type) -> Result<Self, Self::Error> {
        let name = field_type.get_base_type();

        match name.parse()? {
            ValueType::String => Ok(IdType::String),
            ValueType::Bytes => Ok(IdType::Bytes),
            ValueType::Int8 => Ok(IdType::Int8),
            _ => Err(anyhow!(
                "The `id` field has type `{}` but only `String`, `Bytes`, `Int8`, and `ID` are allowed",
                &name
            )
            .into()),
        }
    }
}

impl std::fmt::Display for IdType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Values for the ids of entities
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Id {
    String(Word),
    Bytes(scalar::Bytes),
    Int8(i64),
}

impl Id {
    pub fn id_type(&self) -> IdType {
        match self {
            Id::String(_) => IdType::String,
            Id::Bytes(_) => IdType::Bytes,
            Id::Int8(_) => IdType::Int8,
        }
    }
}

impl std::hash::Hash for Id {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        core::mem::discriminant(self).hash(state);
        match self {
            Id::String(s) => s.hash(state),
            Id::Bytes(b) => b.hash(state),
            Id::Int8(i) => i.hash(state),
        }
    }
}

impl PartialEq<Value> for Id {
    fn eq(&self, other: &Value) -> bool {
        match (self, other) {
            (Id::String(s), Value::String(v)) => s.as_str() == v.as_str(),
            (Id::Bytes(s), Value::Bytes(v)) => s == v,
            (Id::Int8(s), Value::Int8(v)) => s == v,
            _ => false,
        }
    }
}

impl PartialEq<Id> for Value {
    fn eq(&self, other: &Id) -> bool {
        other.eq(self)
    }
}

impl TryFrom<Value> for Id {
    type Error = Error;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        match value {
            Value::String(s) => Ok(Id::String(Word::from(s))),
            Value::Bytes(b) => Ok(Id::Bytes(b)),
            Value::Int8(i) => Ok(Id::Int8(i)),
            _ => Err(anyhow!(
                "expected string or bytes for id but found {:?}",
                value
            )),
        }
    }
}

impl From<Id> for Value {
    fn from(value: Id) -> Self {
        match value {
            Id::String(s) => Value::String(s.into()),
            Id::Bytes(b) => Value::Bytes(b),
            Id::Int8(i) => Value::Int8(i),
        }
    }
}

impl std::fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Id::String(s) => write!(f, "{}", s),
            Id::Bytes(b) => write!(f, "{}", b),
            Id::Int8(i) => write!(f, "{}", i),
        }
    }
}

impl CacheWeight for Id {
    fn indirect_weight(&self) -> usize {
        match self {
            Id::String(s) => s.indirect_weight(),
            Id::Bytes(b) => b.indirect_weight(),
            Id::Int8(_) => 0,
        }
    }
}

impl GasSizeOf for Id {
    fn gas_size_of(&self) -> Gas {
        match self {
            Id::String(s) => s.gas_size_of(),
            Id::Bytes(b) => b.gas_size_of(),
            Id::Int8(i) => i.gas_size_of(),
        }
    }
}

impl StableHash for Id {
    fn stable_hash<H: StableHasher>(&self, field_address: H::Addr, state: &mut H) {
        match self {
            Id::String(s) => stable_hash::StableHash::stable_hash(s, field_address, state),
            Id::Bytes(b) => {
                // We have to convert here to a string `0xdeadbeef` for
                // backwards compatibility. It would be nice to avoid that
                // allocation and just use the bytes directly, but that will
                // break PoI compatibility
                stable_hash::StableHash::stable_hash(&b.to_string(), field_address, state)
            }
            Id::Int8(i) => stable_hash::StableHash::stable_hash(i, field_address, state),
        }
    }
}

impl stable_hash_legacy::StableHash for Id {
    fn stable_hash<H: stable_hash_legacy::StableHasher>(
        &self,
        sequence_number: H::Seq,
        state: &mut H,
    ) {
        match self {
            Id::String(s) => stable_hash_legacy::StableHash::stable_hash(s, sequence_number, state),
            Id::Bytes(b) => {
                stable_hash_legacy::StableHash::stable_hash(&b.to_string(), sequence_number, state)
            }
            Id::Int8(i) => stable_hash_legacy::StableHash::stable_hash(i, sequence_number, state),
        }
    }
}

/// A value that contains a reference to the underlying data for an entity
/// ID. This is used to avoid cloning the ID when it is not necessary.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum IdRef<'a> {
    String(&'a str),
    Bytes(&'a [u8]),
    Int8(i64),
}

impl std::fmt::Display for IdRef<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IdRef::String(s) => write!(f, "{}", s),
            IdRef::Bytes(b) => write!(f, "0x{}", hex::encode(b)),
            IdRef::Int8(i) => write!(f, "{}", i),
        }
    }
}

impl<'a> IdRef<'a> {
    pub fn to_value(self) -> Id {
        match self {
            IdRef::String(s) => Id::String(Word::from(s.to_owned())),
            IdRef::Bytes(b) => Id::Bytes(scalar::Bytes::from(b)),
            IdRef::Int8(i) => Id::Int8(i),
        }
    }

    fn id_type(&self) -> IdType {
        match self {
            IdRef::String(_) => IdType::String,
            IdRef::Bytes(_) => IdType::Bytes,
            IdRef::Int8(_) => IdType::Int8,
        }
    }
}

/// A homogeneous list of entity ids, i.e., all ids in the list are of the
/// same `IdType`
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IdList {
    String(Vec<Word>),
    Bytes(Vec<scalar::Bytes>),
    Int8(Vec<i64>),
}

impl IdList {
    pub fn new(typ: IdType) -> Self {
        match typ {
            IdType::String => IdList::String(Vec::new()),
            IdType::Bytes => IdList::Bytes(Vec::new()),
            IdType::Int8 => IdList::Int8(Vec::new()),
        }
    }

    pub fn len(&self) -> usize {
        match self {
            IdList::String(ids) => ids.len(),
            IdList::Bytes(ids) => ids.len(),
            IdList::Int8(ids) => ids.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn id_type(&self) -> IdType {
        match self {
            IdList::String(_) => IdType::String,
            IdList::Bytes(_) => IdType::Bytes,
            IdList::Int8(_) => IdType::Int8,
        }
    }

    /// Turn a list of ids into an `IdList` and check that they are all the
    /// same type
    pub fn try_from_iter<I: Iterator<Item = Id>>(
        entity_type: &EntityType,
        mut iter: I,
    ) -> Result<Self, QueryExecutionError> {
        let id_type = entity_type.id_type()?;
        match id_type {
            IdType::String => {
                let ids: Vec<Word> = iter.try_fold(vec![], |mut ids, id| match id {
                    Id::String(id) => {
                        ids.push(id);
                        Ok(ids)
                    }
                    _ => Err(constraint_violation!(
                        "expected string id, got {}: {}",
                        id.id_type(),
                        id,
                    )),
                })?;
                Ok(IdList::String(ids))
            }
            IdType::Bytes => {
                let ids: Vec<scalar::Bytes> = iter.try_fold(vec![], |mut ids, id| match id {
                    Id::Bytes(id) => {
                        ids.push(id);
                        Ok(ids)
                    }
                    _ => Err(constraint_violation!(
                        "expected bytes id, got {}: {}",
                        id.id_type(),
                        id,
                    )),
                })?;
                Ok(IdList::Bytes(ids))
            }
            IdType::Int8 => {
                let ids: Vec<i64> = iter.try_fold(vec![], |mut ids, id| match id {
                    Id::Int8(id) => {
                        ids.push(id);
                        Ok(ids)
                    }
                    _ => Err(constraint_violation!(
                        "expected int8 id, got {}: {}",
                        id.id_type(),
                        id,
                    )),
                })?;
                Ok(IdList::Int8(ids))
            }
        }
    }

    /// Turn a list of references to ids into an `IdList` and check that
    /// they are all the same type. Note that this method clones all the ids
    /// and `try_from_iter` is therefore preferrable
    pub fn try_from_iter_ref<'a, I: Iterator<Item = IdRef<'a>>>(
        mut iter: I,
    ) -> Result<Self, QueryExecutionError> {
        let first = match iter.next() {
            Some(id) => id,
            None => return Ok(IdList::String(Vec::new())),
        };
        match first {
            IdRef::String(s) => {
                let ids: Vec<_> = iter.try_fold(vec![Word::from(s)], |mut ids, id| match id {
                    IdRef::String(id) => {
                        ids.push(Word::from(id));
                        Ok(ids)
                    }
                    _ => Err(constraint_violation!(
                        "expected string id, got {}: 0x{}",
                        id.id_type(),
                        id,
                    )),
                })?;
                Ok(IdList::String(ids))
            }
            IdRef::Bytes(b) => {
                let ids: Vec<_> =
                    iter.try_fold(vec![scalar::Bytes::from(b)], |mut ids, id| match id {
                        IdRef::Bytes(id) => {
                            ids.push(scalar::Bytes::from(id));
                            Ok(ids)
                        }
                        _ => Err(constraint_violation!(
                            "expected bytes id, got {}: {}",
                            id.id_type(),
                            id,
                        )),
                    })?;
                Ok(IdList::Bytes(ids))
            }
            IdRef::Int8(i) => {
                let ids: Vec<_> = iter.try_fold(vec![i], |mut ids, id| match id {
                    IdRef::Int8(id) => {
                        ids.push(id);
                        Ok(ids)
                    }
                    _ => Err(constraint_violation!(
                        "expected int8 id, got {}: {}",
                        id.id_type(),
                        id,
                    )),
                })?;
                Ok(IdList::Int8(ids))
            }
        }
    }

    pub fn index(&self, index: usize) -> IdRef<'_> {
        match self {
            IdList::String(ids) => IdRef::String(&ids[index]),
            IdList::Bytes(ids) => IdRef::Bytes(ids[index].as_slice()),
            IdList::Int8(ids) => IdRef::Int8(ids[index]),
        }
    }

    pub fn first(&self) -> Option<IdRef<'_>> {
        if self.len() > 0 {
            Some(self.index(0))
        } else {
            None
        }
    }

    pub fn iter(&self) -> Box<dyn Iterator<Item = IdRef<'_>> + '_> {
        match self {
            IdList::String(ids) => Box::new(ids.iter().map(|id| IdRef::String(id))),
            IdList::Bytes(ids) => Box::new(ids.iter().map(|id| IdRef::Bytes(id))),
            IdList::Int8(ids) => Box::new(ids.iter().map(|id| IdRef::Int8(*id))),
        }
    }

    pub fn as_unique(self) -> Self {
        match self {
            IdList::String(mut ids) => {
                ids.sort_unstable();
                ids.dedup();
                IdList::String(ids)
            }
            IdList::Bytes(mut ids) => {
                ids.sort_unstable_by(|id1, id2| id1.as_slice().cmp(id2.as_slice()));
                ids.dedup();
                IdList::Bytes(ids)
            }
            IdList::Int8(mut ids) => {
                ids.sort_unstable();
                ids.dedup();
                IdList::Int8(ids)
            }
        }
    }

    pub fn push(&mut self, entity_id: Id) -> Result<(), StoreError> {
        match (self, entity_id) {
            (IdList::String(ids), Id::String(id)) => {
                ids.push(id);
                Ok(())
            }
            (IdList::Bytes(ids), Id::Bytes(id)) => {
                ids.push(id);
                Ok(())
            }
            (IdList::Int8(ids), Id::Int8(id)) => {
                ids.push(id);
                Ok(())
            }
            (list, id) => Err(constraint_violation!(
                "expected id of type {}, but got {}[{}]",
                list.id_type(),
                id.id_type(),
                id
            )),
        }
    }

    pub fn as_ids(self) -> Vec<Id> {
        match self {
            IdList::String(ids) => ids.into_iter().map(Id::String).collect(),
            IdList::Bytes(ids) => ids.into_iter().map(Id::Bytes).collect(),
            IdList::Int8(ids) => ids.into_iter().map(Id::Int8).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::data::store::{Id, IdType};

    #[test]
    fn generate_id() {
        let id = IdType::Bytes.generate_id(1, 2).unwrap();
        let exp = IdType::Bytes.parse("0x0000000100000002".into()).unwrap();
        assert_eq!(exp, id);

        let id = IdType::Bytes.generate_id(3, 2).unwrap();
        let exp = IdType::Bytes.parse("0x0000000300000002".into()).unwrap();
        assert_eq!(exp, id);

        let id = IdType::Int8.generate_id(3, 2).unwrap();
        let exp = Id::Int8(0x0000_0003__0000_0002);
        assert_eq!(exp, id);

        // Should be id + 1
        let id2 = IdType::Int8.generate_id(3, 3).unwrap();
        let d = id2.to_string().parse::<i64>().unwrap() - id.to_string().parse::<i64>().unwrap();
        assert_eq!(1, d);
        // Should be id + 2^32
        let id3 = IdType::Int8.generate_id(4, 2).unwrap();
        let d = id3.to_string().parse::<i64>().unwrap() - id.to_string().parse::<i64>().unwrap();
        assert_eq!(1 << 32, d);

        IdType::String.generate_id(3, 2).unwrap_err();
    }
}
