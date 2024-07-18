//! The primitives to work with storage in transactional mode.

use crate::{
    blueprint::{
        avl_merkle,
        btree_merkle::FromPrefix,
    },
    codec::{
        Encode,
        Encoder,
    },
    kv_store::{
        StorageColumn,
        TypedWriteOperation,
        WriteOperation,
    },
    storage_interlayer::Interlayer,
    transactional::{
        Changes,
        Modifiable,
        ReferenceBytesKey,
        TableChange,
        TypedStorageTransaction,
    },
    Error as StorageError,
    Mappable,
    Result as StorageResult,
    StorageAsMut,
    StorageAsRef,
    StorageInspect,
    StorageRead,
    StorageSize,
    StorageWrite,
};
use fuel_core_types::fuel_merkle::{
    avl::AVLStorage,
    btree::{
        BTreeStorage,
        MerkleTreeError,
        StorageNode,
    },
    common::Bytes32,
};
use fuel_vm_private::{
    fuel_merkle::avl,
    prelude::StorageMutate,
};
use std::{
    any::{
        Any,
        TypeId,
    },
    borrow::Cow,
    collections::{
        hash_map,
        BTreeMap,
        HashMap,
    },
    ops::{
        Deref,
        DerefMut,
    },
};

pub struct TypedChanges(HashMap<TypeId, Box<dyn TypedTable>>);

pub struct TypedInMemoryTransaction<Storage> {
    tables: HashMap<TypeId, Box<dyn TypedTable>>,
    storage: Storage,
}

impl<Storage> TypedInMemoryTransaction<Storage> {
    pub fn new(storage: Storage) -> Self {
        Self {
            tables: HashMap::new(),
            storage,
        }
    }

    fn table<M>(&self) -> Option<&Table<M>>
    where
        M: Mappable + 'static,
        M::OwnedKey: Ord,
    {
        let type_id = TypeId::of::<M>();
        self.tables
            .get(&type_id)
            .and_then(|table| table.downcast_ref())
    }

    fn mut_table<M>(&mut self) -> &mut Table<M>
    where
        M: Interlayer + 'static,
        M::OwnedKey: Ord,
    {
        let type_id = TypeId::of::<M>();

        self.tables.entry(type_id).or_insert_with(|| {
            let new_table = Table::<M>::new();
            Box::new(new_table)
        });

        self.tables
            .get_mut(&type_id)
            .expect("We've checked existence above")
            .downcast_mut()
            .expect("We've inserted the correct type above")
    }

    fn merge(&mut self, other: TypedChanges) {
        for (type_id, table) in other.0 {
            let entry = self.tables.entry(type_id);

            match entry {
                hash_map::Entry::Occupied(occupied) => {
                    occupied
                        .into_mut()
                        .merge(table)
                        .expect("The `type_id` of `Self` is the same as in `other`");
                }
                hash_map::Entry::Vacant(vacant) => {
                    vacant.insert(table);
                }
            }
        }
    }

    pub fn into_typed_changes_and_storage(self) -> (TypedChanges, Storage) {
        (TypedChanges(self.tables), self.storage)
    }

    pub fn into_changes_and_storage(self) -> (Changes, Storage) {
        let mut changes = Changes::new();
        for (_, table) in self.tables {
            let (column, tree) = table.table_change();
            let _old = changes.insert(column, tree);
            debug_assert!(_old.is_none());
        }
        (changes, self.storage)
    }
}

impl<S> TypedStorageTransaction<S> {
    pub fn commit_typed_changes(&mut self, changes: TypedChanges) {
        self.inner.merge(changes);
    }
}

impl<S> TypedStorageTransaction<TypedStorageTransaction<S>> {
    pub fn commit(self) -> TypedStorageTransaction<S> {
        let (changes, mut storage) = self.inner.into_typed_changes_and_storage();
        storage.inner.merge(changes);
        storage
    }
}

impl<S> TypedStorageTransaction<&mut TypedStorageTransaction<S>> {
    pub fn commit(self) {
        let (changes, storage) = self.inner.into_typed_changes_and_storage();
        storage.inner.merge(changes);
    }
}

impl<S> TypedStorageTransaction<S>
where
    S: Modifiable,
{
    pub fn commit(self) -> StorageResult<()> {
        let (changes, mut storage) = self.inner.into_changes_and_storage();
        storage.commit_changes(changes)
    }
}

impl<S> From<TypedStorageTransaction<S>> for Changes {
    fn from(value: TypedStorageTransaction<S>) -> Self {
        let (changes, _) = value.inner.into_changes_and_storage();
        changes
    }
}

impl<M, Storage> StorageInspect<M> for TypedInMemoryTransaction<Storage>
where
    M: Mappable + 'static,
    M::OwnedKey: Ord,
    Storage: StorageInspect<M>,
{
    type Error = Storage::Error;

    fn get(&self, key: &M::Key) -> Result<Option<Cow<M::OwnedValue>>, Self::Error> {
        let table = self.table::<M>();

        if let Some(table) = table {
            let key = key.to_owned().into();
            if let Some(value) = table.get(&key) {
                return match value {
                    TypedWriteOperation::Insert(value) => Ok(Some(Cow::Borrowed(value))),
                    TypedWriteOperation::Remove => Ok(None),
                };
            }
        }

        self.storage.get(key)
    }

    fn contains_key(&self, key: &M::Key) -> Result<bool, Self::Error> {
        let table = self.table::<M>();

        if let Some(table) = table {
            let key = key.to_owned().into();
            if let Some(value) = table.get(&key) {
                return match value {
                    TypedWriteOperation::Insert(_) => Ok(true),
                    TypedWriteOperation::Remove => Ok(false),
                };
            }
        }

        self.storage.contains_key(key)
    }
}

impl<M, Storage> StorageMutate<M> for TypedInMemoryTransaction<Storage>
where
    M: Interlayer + 'static,
    M::OwnedKey: Ord,
    Storage: StorageInspect<M>,
{
    fn insert(&mut self, key: &M::Key, value: &M::Value) -> Result<(), Self::Error> {
        let table = self.mut_table::<M>();

        table.insert(
            key.to_owned().into(),
            TypedWriteOperation::Insert(value.to_owned().into()),
        );
        Ok(())
    }

    fn replace(
        &mut self,
        key: &M::Key,
        value: &M::Value,
    ) -> Result<Option<M::OwnedValue>, Self::Error> {
        let table = self.mut_table::<M>();

        let prev = table.insert(
            key.to_owned().into(),
            TypedWriteOperation::Insert(value.to_owned().into()),
        );

        match prev {
            Some(TypedWriteOperation::Insert(value)) => Ok(Some(value)),
            Some(TypedWriteOperation::Remove) => Ok(None),
            None => {
                let prev = self.storage.get(key)?;

                Ok(prev.map(|v| v.into_owned()))
            }
        }
    }

    fn remove(&mut self, key: &M::Key) -> Result<(), Self::Error> {
        let table = self.mut_table::<M>();

        table.insert(key.to_owned().into(), TypedWriteOperation::Remove);
        Ok(())
    }

    fn take(&mut self, key: &M::Key) -> Result<Option<M::OwnedValue>, Self::Error> {
        let table = self.mut_table::<M>();

        let prev = table.insert(key.to_owned().into(), TypedWriteOperation::Remove);

        match prev {
            Some(TypedWriteOperation::Insert(value)) => Ok(Some(value)),
            Some(TypedWriteOperation::Remove) => Ok(None),
            None => {
                let prev = self.storage.get(key)?;

                Ok(prev.map(|v| v.into_owned()))
            }
        }
    }
}

impl<M, Storage> StorageSize<M> for TypedInMemoryTransaction<Storage>
where
    M: Interlayer + 'static,
    M::OwnedKey: Ord,
    M::OwnedValue: AsRef<[u8]>,
    Storage: StorageSize<M>,
{
    fn size_of_value(&self, key: &M::Key) -> Result<Option<usize>, Self::Error> {
        let table = self.table::<M>();

        if let Some(table) = table {
            let key = key.to_owned().into();
            if let Some(value) = table.get(&key) {
                return match value {
                    TypedWriteOperation::Insert(value) => Ok(Some(value.as_ref().len())),
                    TypedWriteOperation::Remove => Ok(None),
                };
            }
        }

        self.storage.size_of_value(key)
    }
}

impl<M, Storage> StorageRead<M> for TypedInMemoryTransaction<Storage>
where
    M: Interlayer + 'static,
    M::OwnedKey: Ord,
    M::OwnedValue: AsRef<[u8]>,
    Storage: StorageRead<M, Error = StorageError>,
{
    fn read(&self, key: &M::Key, buf: &mut [u8]) -> Result<Option<usize>, Self::Error> {
        let table = self.table::<M>();

        if let Some(table) = table {
            let key = key.to_owned().into();
            if let Some(value) = table.get(&key) {
                return match value {
                    TypedWriteOperation::Insert(value) => {
                        let read = value.as_ref().len();
                        if read != buf.len() {
                            return Err(StorageError::Other(anyhow::anyhow!(
                                "Buffer size is not equal to the value size"
                            )));
                        }
                        buf.copy_from_slice(value.as_ref());
                        Ok(Some(read))
                    }
                    TypedWriteOperation::Remove => Ok(None),
                };
            }
        }

        self.storage.read(key, buf)
    }

    fn read_alloc(&self, key: &M::Key) -> Result<Option<Vec<u8>>, Self::Error> {
        let table = self.table::<M>();

        if let Some(table) = table {
            let key = key.to_owned().into();
            if let Some(value) = table.get(&key) {
                return match value {
                    TypedWriteOperation::Insert(value) => {
                        Ok(Some(value.as_ref().to_vec()))
                    }
                    TypedWriteOperation::Remove => Ok(None),
                };
            }
        }

        self.storage.read_alloc(key)
    }
}

impl<M, Storage> StorageWrite<M> for TypedInMemoryTransaction<Storage>
where
    M: Interlayer<Value = [u8]> + 'static,
    M::OwnedKey: Ord,
    M::OwnedValue: Into<Vec<u8>>,
    Storage: StorageRead<M>,
{
    fn write_bytes(&mut self, key: &M::Key, buf: &[u8]) -> Result<usize, Self::Error> {
        let written_bytes = buf.len();
        let table = self.mut_table::<M>();

        table.insert(
            key.to_owned().into(),
            TypedWriteOperation::Insert(buf.to_owned().into()),
        );
        Ok(written_bytes)
    }

    fn replace_bytes(
        &mut self,
        key: &M::Key,
        buf: &[u8],
    ) -> Result<(usize, Option<Vec<u8>>), Self::Error> {
        let table = self.mut_table::<M>();

        let bytes_written = buf.len();
        let prev = table.insert(
            key.to_owned().into(),
            TypedWriteOperation::Insert(buf.to_owned().into()),
        );

        match prev {
            Some(TypedWriteOperation::Insert(value)) => {
                Ok((bytes_written, Some(value.into())))
            }
            Some(TypedWriteOperation::Remove) => Ok((bytes_written, None)),
            None => Ok((bytes_written, self.storage.read_alloc(key)?)),
        }
    }

    fn take_bytes(&mut self, key: &M::Key) -> Result<Option<Vec<u8>>, Self::Error> {
        let table = self.mut_table::<M>();

        let prev = table.insert(key.to_owned().into(), TypedWriteOperation::Remove);

        match prev {
            Some(TypedWriteOperation::Insert(value)) => Ok(Some(value.into())),
            Some(TypedWriteOperation::Remove) => Ok(None),
            None => self.storage.read_alloc(key),
        }
    }
}

impl<Storage, M> BTreeStorage<M> for TypedInMemoryTransaction<Storage>
where
    M: Interlayer<Value = StorageNode, OwnedValue = StorageNode> + 'static,
    M::Key: Sized + FromPrefix,
    M::OwnedKey: Ord,
    Self: StorageMutate<M, Error = StorageError>,
{
    type StorageError = StorageError;

    fn take(
        &mut self,
        prefix: &Bytes32,
        key: &Bytes32,
    ) -> Result<Option<StorageNode>, MerkleTreeError<Self::StorageError>> {
        let storage_key = M::Key::from_prefix(*prefix, *key);
        let value = self
            .storage_as_mut::<M>()
            .take(&storage_key)
            .map_err(MerkleTreeError::StorageError)?;
        Ok(value)
    }

    fn set(
        &mut self,
        prefix: &Bytes32,
        key: &Bytes32,
        value: StorageNode,
    ) -> Result<(), MerkleTreeError<Self::StorageError>> {
        let storage_key = M::Key::from_prefix(*prefix, *key).to_owned().into();
        let table = self.mut_table::<M>();

        table.insert(storage_key, TypedWriteOperation::Insert(value));
        Ok(())
    }
}

impl<Storage, M> AVLStorage<M> for TypedInMemoryTransaction<Storage>
where
    M: Interlayer<Value = avl::StorageNode, OwnedValue = avl::StorageNode> + 'static,
    M::Key: Sized + avl_merkle::FromPrefix,
    M::OwnedKey: Ord,
    Self: StorageMutate<M, Error = StorageError>,
{
    type StorageError = StorageError;

    fn get(
        &self,
        prefix: &Bytes32,
        key: &Bytes32,
    ) -> Result<Option<avl::StorageNode>, avl::MerkleTreeError<Self::StorageError>> {
        use avl_merkle::FromPrefix;
        let storage_key = M::Key::from_prefix(*prefix, *key);
        let value = self
            .storage_as_ref::<M>()
            .get(&storage_key)
            .map_err(avl::MerkleTreeError::StorageError)?
            .map(|value| value.into_owned());
        Ok(value)
    }

    fn set(
        &mut self,
        prefix: &Bytes32,
        key: &Bytes32,
        value: &avl::StorageNode,
    ) -> Result<(), avl::MerkleTreeError<Self::StorageError>> {
        use avl_merkle::FromPrefix;
        let storage_key = M::Key::from_prefix(*prefix, *key).to_owned().into();
        let table = self.mut_table::<M>();

        table.insert(storage_key, TypedWriteOperation::Insert(value.clone()));
        Ok(())
    }
}

struct Table<M>
where
    M: Mappable,
{
    map: BTreeMap<M::OwnedKey, TypedWriteOperation<M::OwnedValue>>,
}

impl<M> Default for Table<M>
where
    M: Mappable,
{
    fn default() -> Self {
        Self {
            map: BTreeMap::new(),
        }
    }
}

impl<M> Deref for Table<M>
where
    M: Mappable,
{
    type Target = BTreeMap<M::OwnedKey, TypedWriteOperation<M::OwnedValue>>;

    fn deref(&self) -> &Self::Target {
        &self.map
    }
}

impl<M> DerefMut for Table<M>
where
    M: Mappable,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.map
    }
}

impl<M> Table<M>
where
    M: Mappable,
{
    pub fn new() -> Self {
        Self::default()
    }
}

#[derive(Debug)]
struct MismatchedType;

trait TypedTable: Any {
    fn merge(&mut self, other: Box<dyn TypedTable>) -> Result<(), MismatchedType>;

    fn table_change(&self) -> TableChange;

    fn self_type_id(&self) -> TypeId {
        Any::type_id(self)
    }
}

impl dyn TypedTable {
    #[inline]
    pub fn is<T: Any>(&self) -> bool {
        // Get `TypeId` of the type this function is instantiated with.
        let t = TypeId::of::<T>();

        // Get `TypeId` of the type in the trait object (`self`).
        let concrete = self.self_type_id();

        // Compare both `TypeId`s on equality.
        t == concrete
    }

    #[inline]
    pub fn downcast_ref<T: Any>(&self) -> Option<&T> {
        if self.is::<T>() {
            // SAFETY: just checked whether we are pointing to the correct type, and we can rely on
            // that check for memory safety because we have implemented Any for all types; no other
            // impls can exist as they would conflict with our impl.
            unsafe { Some(self.downcast_ref_unchecked()) }
        } else {
            None
        }
    }

    #[inline]
    pub fn downcast_mut<T: Any>(&mut self) -> Option<&mut T> {
        if self.is::<T>() {
            // SAFETY: just checked whether we are pointing to the correct type, and we can rely on
            // that check for memory safety because we have implemented Any for all types; no other
            // impls can exist as they would conflict with our impl.
            unsafe { Some(self.downcast_mut_unchecked()) }
        } else {
            None
        }
    }

    #[inline]
    pub unsafe fn downcast_ref_unchecked<T: Any>(&self) -> &T {
        debug_assert!(self.is::<T>());
        // SAFETY: caller guarantees that T is the correct type
        unsafe { &*(self as *const dyn TypedTable as *const T) }
    }

    #[inline]
    pub unsafe fn downcast_mut_unchecked<T: Any>(&mut self) -> &mut T {
        debug_assert!(self.is::<T>());
        // SAFETY: caller guarantees that T is the correct type
        unsafe { &mut *(self as *mut dyn TypedTable as *mut T) }
    }
}

impl<M> TypedTable for Table<M>
where
    M: Interlayer + 'static,
    M::OwnedKey: Ord,
{
    fn merge(&mut self, mut other: Box<dyn TypedTable>) -> Result<(), MismatchedType> {
        let btree = other.downcast_mut::<Table<M>>().ok_or(MismatchedType)?;
        let extract = core::mem::take(btree.deref_mut());

        for (key, value) in extract {
            self.insert(key, value);
        }
        Ok(())
    }

    fn table_change(&self) -> TableChange {
        let column = M::column().id();
        let tree = self
            .map
            .iter()
            .map(|(key, value)| {
                let encoder = M::KeyCodec::encode(key);
                let key: ReferenceBytesKey = encoder.as_bytes().into_owned().into();
                let value = match value {
                    TypedWriteOperation::Insert(value) => {
                        let value = M::ValueCodec::encode_as_value(value);
                        WriteOperation::Insert(value)
                    }
                    TypedWriteOperation::Remove => WriteOperation::Remove,
                };

                (key, value)
            })
            .collect();

        (column, tree)
    }
}

#[cfg(test)]
mod test {
    use crate::{
        column::Column,
        structured_storage::test::InMemoryStorage,
        transactional::{
            IntoTransaction,
            WriteTransaction,
        },
        StorageAsMut,
        StorageAsRef,
    };
    use fuel_core_types::fuel_types::Nonce;

    #[test]
    fn dummy() {
        use crate::tables::Messages;
        let mut storage = InMemoryStorage::<Column>::default().into_transaction();
        let mut mapping_1 = storage.write_typed_transaction();
        mapping_1
            .storage_as_mut::<Messages>()
            .insert(&Nonce::zeroed(), &Default::default())
            .unwrap();

        assert!(mapping_1
            .storage_as_ref::<Messages>()
            .get(&Nonce::zeroed())
            .unwrap()
            .is_some());

        let mut mapping_2 = mapping_1.write_typed_transaction();
        mapping_2
            .storage_as_mut::<Messages>()
            .remove(&Nonce::zeroed())
            .unwrap();
        mapping_2.commit();
        assert!(mapping_1
            .storage_as_ref::<Messages>()
            .get(&Nonce::zeroed())
            .unwrap()
            .is_none());
    }
}
