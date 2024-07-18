use crate::{
    codec::{
        Decode,
        Encode,
        Encoder,
    },
    iter::{
        BoxedIter,
        IterDirection,
        IterableStore,
    },
    kv_store::{
        BatchOperations,
        KVItem,
        KeyValueInspect,
        KeyValueMutate,
        StorageColumn,
        Value,
        WriteOperation,
    },
    transactional::{
        Changes,
        Modifiable,
    },
    Error as StorageError,
    Mappable,
    Result as StorageResult,
    StorageInspect,
    StorageMutate,
    StorageRead,
    StorageSize,
    StorageWrite,
};
use std::borrow::Cow;

pub trait Interlayer: Mappable + Sized {
    /// The codec used to encode and decode storage key.
    type KeyCodec: Encode<Self::Key> + Encode<Self::OwnedKey> + Decode<Self::OwnedKey>;
    /// The codec used to encode and decode storage value.
    type ValueCodec: Encode<Self::Value>
        + Encode<Self::OwnedValue>
        + Decode<Self::OwnedValue>;

    /// The column type used by the table.
    type Column: StorageColumn;

    /// The column occupied by the table.
    fn column() -> Self::Column;
}

/// The storage interlayer that implements storage traits on top of the raw key value storage.
#[derive(Default, Debug, Clone)]
pub struct StorageInterlayer<S> {
    pub(crate) inner: S,
}

impl<S> StorageInterlayer<S> {
    /// Creates a new instance of the structured storage.
    pub fn new(storage: S) -> Self {
        Self { inner: storage }
    }

    /// Returns the inner storage.
    pub fn into_inner(self) -> S {
        self.inner
    }
}

impl<S> AsRef<S> for StorageInterlayer<S> {
    fn as_ref(&self) -> &S {
        &self.inner
    }
}

impl<S> AsMut<S> for StorageInterlayer<S> {
    fn as_mut(&mut self) -> &mut S {
        &mut self.inner
    }
}

impl<S> KeyValueInspect for StorageInterlayer<S>
where
    S: KeyValueInspect,
{
    type Column = S::Column;

    fn exists(&self, key: &[u8], column: Self::Column) -> StorageResult<bool> {
        self.inner.exists(key, column)
    }

    fn size_of_value(
        &self,
        key: &[u8],
        column: Self::Column,
    ) -> StorageResult<Option<usize>> {
        self.inner.size_of_value(key, column)
    }

    fn get(&self, key: &[u8], column: Self::Column) -> StorageResult<Option<Value>> {
        self.inner.get(key, column)
    }

    fn read(
        &self,
        key: &[u8],
        column: Self::Column,
        buf: &mut [u8],
    ) -> StorageResult<Option<usize>> {
        self.inner.read(key, column, buf)
    }
}

impl<S> KeyValueMutate for StorageInterlayer<S>
where
    S: KeyValueMutate,
{
    fn put(
        &mut self,
        key: &[u8],
        column: Self::Column,
        value: Value,
    ) -> StorageResult<()> {
        self.inner.put(key, column, value)
    }

    fn replace(
        &mut self,
        key: &[u8],
        column: Self::Column,
        value: Value,
    ) -> StorageResult<Option<Value>> {
        self.inner.replace(key, column, value)
    }

    fn write(
        &mut self,
        key: &[u8],
        column: Self::Column,
        buf: &[u8],
    ) -> StorageResult<usize> {
        self.inner.write(key, column, buf)
    }

    fn take(&mut self, key: &[u8], column: Self::Column) -> StorageResult<Option<Value>> {
        self.inner.take(key, column)
    }

    fn delete(&mut self, key: &[u8], column: Self::Column) -> StorageResult<()> {
        self.inner.delete(key, column)
    }
}

impl<S> BatchOperations for StorageInterlayer<S>
where
    S: BatchOperations,
{
    fn batch_write<I>(&mut self, column: Self::Column, entries: I) -> StorageResult<()>
    where
        I: Iterator<Item = (Vec<u8>, WriteOperation)>,
    {
        self.inner.batch_write(column, entries)
    }
}

impl<S> IterableStore for StorageInterlayer<S>
where
    S: IterableStore,
{
    fn iter_store(
        &self,
        column: Self::Column,
        prefix: Option<&[u8]>,
        start: Option<&[u8]>,
        direction: IterDirection,
    ) -> BoxedIter<KVItem> {
        self.inner.iter_store(column, prefix, start, direction)
    }
}

impl<S> Modifiable for StorageInterlayer<S>
where
    S: Modifiable,
{
    fn commit_changes(&mut self, changes: Changes) -> StorageResult<()> {
        self.inner.commit_changes(changes)
    }
}

impl<Column, S, M> StorageInspect<M> for StorageInterlayer<S>
where
    S: KeyValueInspect<Column = Column>,
    M: Interlayer<Column = Column>,
{
    type Error = StorageError;

    fn get(&self, key: &M::Key) -> Result<Option<Cow<M::OwnedValue>>, Self::Error> {
        let key_encoder = M::KeyCodec::encode(key);
        let key_bytes = key_encoder.as_bytes();
        let value = self
            .inner
            .get(key_bytes.as_ref(), M::column())?
            .map(|value| {
                M::ValueCodec::decode_from_value(value).map_err(crate::Error::Codec)
            })
            .transpose()?;

        Ok(value.map(Cow::Owned))
    }

    fn contains_key(&self, key: &M::Key) -> Result<bool, Self::Error> {
        let key_encoder = M::KeyCodec::encode(key);
        let key_bytes = key_encoder.as_bytes();
        self.inner.exists(key_bytes.as_ref(), M::column())
    }
}

impl<Column, S, M> StorageMutate<M> for StorageInterlayer<S>
where
    S: KeyValueMutate<Column = Column>,
    M: Interlayer<Column = Column>,
{
    fn insert(&mut self, key: &M::Key, value: &M::Value) -> Result<(), Self::Error> {
        let key_encoder = M::KeyCodec::encode(key);
        let key_bytes = key_encoder.as_bytes();
        let value = M::ValueCodec::encode_as_value(value);
        self.inner.put(key_bytes.as_ref(), M::column(), value)
    }

    fn replace(
        &mut self,
        key: &M::Key,
        value: &M::Value,
    ) -> Result<Option<M::OwnedValue>, Self::Error> {
        let key_encoder = M::KeyCodec::encode(key);
        let key_bytes = key_encoder.as_bytes();
        let value = M::ValueCodec::encode_as_value(value);
        self.inner
            .replace(key_bytes.as_ref(), M::column(), value)?
            .map(|value| {
                M::ValueCodec::decode_from_value(value).map_err(StorageError::Codec)
            })
            .transpose()
    }

    fn remove(&mut self, key: &M::Key) -> Result<(), Self::Error> {
        let key_encoder = M::KeyCodec::encode(key);
        let key_bytes = key_encoder.as_bytes();
        self.inner.delete(key_bytes.as_ref(), M::column())
    }

    fn take(&mut self, key: &M::Key) -> Result<Option<M::OwnedValue>, Self::Error> {
        let key_encoder = M::KeyCodec::encode(key);
        let key_bytes = key_encoder.as_bytes();
        self.inner
            .take(key_bytes.as_ref(), M::column())?
            .map(|value| {
                M::ValueCodec::decode_from_value(value).map_err(StorageError::Codec)
            })
            .transpose()
    }
}

impl<Column, S, M> StorageSize<M> for StorageInterlayer<S>
where
    S: KeyValueInspect<Column = Column>,
    M: Interlayer<Column = Column, Value = [u8]>,
{
    fn size_of_value(&self, key: &M::Key) -> Result<Option<usize>, Self::Error> {
        let key_encoder = M::KeyCodec::encode(key);
        let key_bytes = key_encoder.as_bytes();
        self.inner.size_of_value(key_bytes.as_ref(), M::column())
    }
}

impl<Column, S, M> StorageRead<M> for StorageInterlayer<S>
where
    S: KeyValueInspect<Column = Column>,
    M: Interlayer<Column = Column, Value = [u8]>,
    M::OwnedValue: Into<Vec<u8>>,
{
    fn read(&self, key: &M::Key, buf: &mut [u8]) -> Result<Option<usize>, Self::Error> {
        let key_encoder = M::KeyCodec::encode(key);
        let key_bytes = key_encoder.as_bytes();
        self.inner.read(key_bytes.as_ref(), M::column(), buf)
    }

    fn read_alloc(&self, key: &M::Key) -> Result<Option<Vec<u8>>, Self::Error> {
        let key_encoder = M::KeyCodec::encode(key);
        let key_bytes = key_encoder.as_bytes();
        let value = self
            .inner
            .get(key_bytes.as_ref(), M::column())?
            .map(|value| {
                M::ValueCodec::decode_from_value(value).map_err(StorageError::Codec)
            })
            .transpose()?;

        Ok(value.map(Into::into))
    }
}

impl<Column, S, M> StorageWrite<M> for StorageInterlayer<S>
where
    S: KeyValueMutate<Column = Column>,
    M: Interlayer<Column = Column, Value = [u8]>,
    M::OwnedValue: Into<Vec<u8>>,
{
    fn write_bytes(&mut self, key: &M::Key, buf: &[u8]) -> Result<usize, Self::Error> {
        let bytes_written = buf.len();
        let key_encoder = M::KeyCodec::encode(key);
        let key_bytes = key_encoder.as_bytes();
        let value = buf.to_vec().into();
        self.put(key_bytes.as_ref(), M::column(), value)?;

        Ok(bytes_written)
    }

    fn replace_bytes(
        &mut self,
        key: &M::Key,
        buf: &[u8],
    ) -> Result<(usize, Option<Vec<u8>>), Self::Error> {
        let bytes_written = buf.len();
        let key_encoder = M::KeyCodec::encode(key);
        let key_bytes = key_encoder.as_bytes();
        let value = buf.to_vec().into();
        let old_bytes =
            KeyValueMutate::replace(self, key_bytes.as_ref(), M::column(), value)?
                .map(|value| {
                    M::ValueCodec::decode_from_value(value).map_err(StorageError::Codec)
                })
                .transpose()?;

        Ok((bytes_written, old_bytes.map(Into::into)))
    }

    fn take_bytes(&mut self, key: &M::Key) -> Result<Option<Vec<u8>>, Self::Error> {
        let key_encoder = M::KeyCodec::encode(key);
        let key_bytes = key_encoder.as_bytes();
        let old_bytes = KeyValueMutate::take(self, key_bytes.as_ref(), M::column())?
            .map(|value| {
                M::ValueCodec::decode_from_value(value).map_err(StorageError::Codec)
            })
            .transpose()?;

        Ok(old_bytes.map(Into::into))
    }
}
