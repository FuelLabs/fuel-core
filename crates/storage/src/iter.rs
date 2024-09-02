//! The module defines primitives that allow iterating of the storage.

use crate::{
    blueprint::BlueprintInspect,
    codec::{
        Decode,
        Encode,
        Encoder,
    },
    kv_store::{
        KVItem,
        KeyItem,
        KeyValueInspect,
    },
    structured_storage::TableWithBlueprint,
    transactional::ReferenceBytesKey,
};
use fuel_vm_private::fuel_storage::Mappable;

#[cfg(feature = "alloc")]
use alloc::{
    boxed::Box,
    collections::BTreeMap,
    vec::Vec,
};

// TODO: BoxedIter to be used until RPITIT lands in stable rust.
/// A boxed variant of the iterator that can be used as a return type of the traits.
pub struct BoxedIter<'a, T> {
    iter: Box<dyn Iterator<Item = T> + 'a>,
}

impl<'a, T> Iterator for BoxedIter<'a, T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next()
    }
}

/// The traits simplifies conversion into `BoxedIter`.
pub trait IntoBoxedIter<'a, T> {
    /// Converts `Self` iterator into `BoxedIter`.
    fn into_boxed(self) -> BoxedIter<'a, T>;
}

impl<'a, T, I> IntoBoxedIter<'a, T> for I
where
    I: Iterator<Item = T> + 'a,
{
    fn into_boxed(self) -> BoxedIter<'a, T> {
        BoxedIter {
            iter: Box::new(self),
        }
    }
}

/// A enum for iterating across the database
#[derive(Copy, Clone, Debug, PartialOrd, Eq, PartialEq)]
pub enum IterDirection {
    /// Iterate forward
    Forward,
    /// Iterate backward
    Reverse,
}

impl Default for IterDirection {
    fn default() -> Self {
        Self::Forward
    }
}

/// A trait for iterating over the storage of [`KeyValueInspect`].
#[impl_tools::autoimpl(for<T: trait> &T, &mut T, Box<T>)]
pub trait IterableStore: KeyValueInspect {
    /// Returns an iterator over the values in the storage.
    fn iter_store(
        &self,
        column: Self::Column,
        prefix: Option<&[u8]>,
        start: Option<&[u8]>,
        direction: IterDirection,
    ) -> BoxedIter<KVItem>;

    /// Returns an iterator over keys in the storage.
    fn iter_store_keys(
        &self,
        column: Self::Column,
        prefix: Option<&[u8]>,
        start: Option<&[u8]>,
        direction: IterDirection,
    ) -> BoxedIter<KeyItem>;
}

#[cfg(feature = "std")]
impl<T> IterableStore for std::sync::Arc<T>
where
    T: IterableStore,
{
    fn iter_store(
        &self,
        column: Self::Column,
        prefix: Option<&[u8]>,
        start: Option<&[u8]>,
        direction: IterDirection,
    ) -> BoxedIter<KVItem> {
        use core::ops::Deref;
        self.deref().iter_store(column, prefix, start, direction)
    }

    fn iter_store_keys(
        &self,
        column: Self::Column,
        prefix: Option<&[u8]>,
        start: Option<&[u8]>,
        direction: IterDirection,
    ) -> BoxedIter<KeyItem> {
        use core::ops::Deref;
        self.deref()
            .iter_store_keys(column, prefix, start, direction)
    }
}

/// A trait for iterating over the `Mappable` table.
pub trait IterableTable<M>
where
    M: Mappable,
{
    /// Returns an iterator over the all keys in the table with a prefix after a specific start key.
    fn iter_table_keys<P>(
        &self,
        prefix: Option<P>,
        start: Option<&M::Key>,
        direction: Option<IterDirection>,
    ) -> BoxedIter<super::Result<M::OwnedKey>>
    where
        P: AsRef<[u8]>;

    /// Returns an iterator over the all entries in the table with a prefix after a specific start key.
    fn iter_table<P>(
        &self,
        prefix: Option<P>,
        start: Option<&M::Key>,
        direction: Option<IterDirection>,
    ) -> BoxedIter<super::Result<(M::OwnedKey, M::OwnedValue)>>
    where
        P: AsRef<[u8]>;
}

impl<Column, M, S> IterableTable<M> for S
where
    M: TableWithBlueprint<Column = Column>,
    M::Blueprint: BlueprintInspect<M, S>,
    S: IterableStore<Column = Column>,
{
    fn iter_table_keys<P>(
        &self,
        prefix: Option<P>,
        start: Option<&M::Key>,
        direction: Option<IterDirection>,
    ) -> BoxedIter<crate::Result<M::OwnedKey>>
    where
        P: AsRef<[u8]>,
    {
        let encoder = start.map(|start| {
            <M::Blueprint as BlueprintInspect<M, Self>>::KeyCodec::encode(start)
        });

        let start = encoder.as_ref().map(|encoder| encoder.as_bytes());

        IterableStore::iter_store_keys(
            self,
            M::column(),
            prefix.as_ref().map(|p| p.as_ref()),
            start.as_ref().map(|cow| cow.as_ref()),
            direction.unwrap_or_default(),
        )
        .map(|res| {
            res.and_then(|key| {
                let key = <M::Blueprint as BlueprintInspect<M, Self>>::KeyCodec::decode(
                    key.as_slice(),
                )
                .map_err(|e| crate::Error::Codec(anyhow::anyhow!(e)))?;
                Ok(key)
            })
        })
        .into_boxed()
    }

    fn iter_table<P>(
        &self,
        prefix: Option<P>,
        start: Option<&M::Key>,
        direction: Option<IterDirection>,
    ) -> BoxedIter<super::Result<(M::OwnedKey, M::OwnedValue)>>
    where
        P: AsRef<[u8]>,
    {
        let encoder = start.map(|start| {
            <M::Blueprint as BlueprintInspect<M, Self>>::KeyCodec::encode(start)
        });

        let start = encoder.as_ref().map(|encoder| encoder.as_bytes());

        IterableStore::iter_store(
            self,
            M::column(),
            prefix.as_ref().map(|p| p.as_ref()),
            start.as_ref().map(|cow| cow.as_ref()),
            direction.unwrap_or_default(),
        )
        .map(|val| {
            val.and_then(|(key, value)| {
                let key = <M::Blueprint as BlueprintInspect<M, Self>>::KeyCodec::decode(
                    key.as_slice(),
                )
                .map_err(|e| crate::Error::Codec(anyhow::anyhow!(e)))?;
                let value =
                    <M::Blueprint as BlueprintInspect<M, Self>>::ValueCodec::decode(
                        value.as_slice(),
                    )
                    .map_err(|e| crate::Error::Codec(anyhow::anyhow!(e)))?;
                Ok((key, value))
            })
        })
        .into_boxed()
    }
}

/// A helper trait to provide a user-friendly API over table iteration.
pub trait IteratorOverTable {
    /// Returns an iterator over the all keys in the table.
    fn iter_all_keys<M>(
        &self,
        direction: Option<IterDirection>,
    ) -> BoxedIter<super::Result<M::OwnedKey>>
    where
        M: Mappable,
        Self: IterableTable<M>,
    {
        self.iter_all_filtered_keys::<M, [u8; 0]>(None, None, direction)
    }

    /// Returns an iterator over the all keys in the table with the specified prefix.
    fn iter_all_by_prefix_keys<M, P>(
        &self,
        prefix: Option<P>,
    ) -> BoxedIter<super::Result<M::OwnedKey>>
    where
        M: Mappable,
        P: AsRef<[u8]>,
        Self: IterableTable<M>,
    {
        self.iter_all_filtered_keys::<M, P>(prefix, None, None)
    }

    /// Returns an iterator over the all keys in the table after a specific start key.
    fn iter_all_by_start_keys<M>(
        &self,
        start: Option<&M::Key>,
        direction: Option<IterDirection>,
    ) -> BoxedIter<super::Result<M::OwnedKey>>
    where
        M: Mappable,
        Self: IterableTable<M>,
    {
        self.iter_all_filtered_keys::<M, [u8; 0]>(None, start, direction)
    }

    /// Returns an iterator over the all keys in the table with a prefix after a specific start key.
    fn iter_all_filtered_keys<M, P>(
        &self,
        prefix: Option<P>,
        start: Option<&M::Key>,
        direction: Option<IterDirection>,
    ) -> BoxedIter<super::Result<M::OwnedKey>>
    where
        M: Mappable,
        P: AsRef<[u8]>,
        Self: IterableTable<M>,
    {
        self.iter_table_keys(prefix, start, direction)
    }

    /// Returns an iterator over the all entries in the table.
    fn iter_all<M>(
        &self,
        direction: Option<IterDirection>,
    ) -> BoxedIter<super::Result<(M::OwnedKey, M::OwnedValue)>>
    where
        M: Mappable,
        Self: IterableTable<M>,
    {
        self.iter_all_filtered::<M, [u8; 0]>(None, None, direction)
    }

    /// Returns an iterator over the all entries in the table with the specified prefix.
    fn iter_all_by_prefix<M, P>(
        &self,
        prefix: Option<P>,
    ) -> BoxedIter<super::Result<(M::OwnedKey, M::OwnedValue)>>
    where
        M: Mappable,
        P: AsRef<[u8]>,
        Self: IterableTable<M>,
    {
        self.iter_all_filtered::<M, P>(prefix, None, None)
    }

    /// Returns an iterator over the all entries in the table after a specific start key.
    fn iter_all_by_start<M>(
        &self,
        start: Option<&M::Key>,
        direction: Option<IterDirection>,
    ) -> BoxedIter<super::Result<(M::OwnedKey, M::OwnedValue)>>
    where
        M: Mappable,
        Self: IterableTable<M>,
    {
        self.iter_all_filtered::<M, [u8; 0]>(None, start, direction)
    }

    /// Returns an iterator over the all entries in the table with a prefix after a specific start key.
    fn iter_all_filtered<M, P>(
        &self,
        prefix: Option<P>,
        start: Option<&M::Key>,
        direction: Option<IterDirection>,
    ) -> BoxedIter<super::Result<(M::OwnedKey, M::OwnedValue)>>
    where
        M: Mappable,
        P: AsRef<[u8]>,
        Self: IterableTable<M>,
    {
        self.iter_table(prefix, start, direction)
    }
}

impl<S> IteratorOverTable for S {}

/// Returns an iterator over the values in the `BTreeMap`.
pub fn iterator<'a, V>(
    tree: &'a BTreeMap<ReferenceBytesKey, V>,
    prefix: Option<&[u8]>,
    start: Option<&[u8]>,
    direction: IterDirection,
) -> impl Iterator<Item = (&'a ReferenceBytesKey, &'a V)> + 'a {
    match (prefix, start) {
        (None, None) => {
            if direction == IterDirection::Forward {
                tree.iter().into_boxed()
            } else {
                tree.iter().rev().into_boxed()
            }
        }
        (Some(prefix), None) => {
            let prefix = prefix.to_vec();
            if direction == IterDirection::Forward {
                tree.range(prefix.clone()..)
                    .take_while(move |(key, _)| key.starts_with(prefix.as_slice()))
                    .into_boxed()
            } else {
                let mut vec: Vec<_> = tree
                    .range(prefix.clone()..)
                    .into_boxed()
                    .take_while(|(key, _)| key.starts_with(prefix.as_slice()))
                    .collect();

                vec.reverse();
                vec.into_iter().into_boxed()
            }
        }
        (None, Some(start)) => {
            if direction == IterDirection::Forward {
                tree.range(start.to_vec()..).into_boxed()
            } else {
                tree.range(..=start.to_vec()).rev().into_boxed()
            }
        }
        (Some(prefix), Some(start)) => {
            let prefix = prefix.to_vec();
            if direction == IterDirection::Forward {
                tree.range(start.to_vec()..)
                    .take_while(move |(key, _)| key.starts_with(prefix.as_slice()))
                    .into_boxed()
            } else {
                tree.range(..=start.to_vec())
                    .rev()
                    .take_while(move |(key, _)| key.starts_with(prefix.as_slice()))
                    .into_boxed()
            }
        }
    }
}

/// Returns an iterator over the keys in the `BTreeMap`.
pub fn keys_iterator<'a, V>(
    tree: &'a BTreeMap<ReferenceBytesKey, V>,
    prefix: Option<&[u8]>,
    start: Option<&[u8]>,
    direction: IterDirection,
) -> impl Iterator<Item = &'a ReferenceBytesKey> + 'a {
    match (prefix, start) {
        (None, None) => {
            if direction == IterDirection::Forward {
                tree.keys().into_boxed()
            } else {
                tree.keys().rev().into_boxed()
            }
        }
        // all the other cases require using a range, so we can't use the keys() method
        (_, _) => iterator(tree, prefix, start, direction)
            .map(|(key, _)| key)
            .into_boxed(),
    }
}
