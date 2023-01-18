use crate::database::{
    Column,
    Database,
};
use fuel_core_storage::{
    tables::FuelBlockRoots,
    Error as StorageError,
    Mappable,
    Result as StorageResult,
    StorageInspect,
    StorageMutate,
};
use fuel_core_types::blockchain::primitives::BlockHeight;
use serde::{
    de::DeserializeOwned,
    Serialize,
};
use std::borrow::Cow;

impl DatabaseColumn for FuelBlockRoots {
    fn column() -> Column {
        Column::BlockHeaderMerkle
    }
}

/// The table has a corresponding column in the database.
trait DatabaseColumn {
    /// The column of the table.
    fn column() -> Column;
}

impl<T> StorageInspect<T> for Database
where
    T: Mappable + DatabaseColumn,
    T::Key: IntoDatabaseKey,
    T::GetValue: DeserializeOwned,
{
    type Error = StorageError;

    fn get(&self, key: &T::Key) -> StorageResult<Option<Cow<T::GetValue>>> {
        let key = key.database_key();
        self.get(key.as_ref(), T::column()).map_err(Into::into)
    }

    fn contains_key(&self, key: &T::Key) -> StorageResult<bool> {
        let key = key.database_key();
        self.exists(key.as_ref(), T::column()).map_err(Into::into)
    }
}

impl<T> StorageMutate<T> for Database
where
    T: Mappable + DatabaseColumn,
    T::Key: IntoDatabaseKey,
    T::SetValue: Serialize,
    T::GetValue: DeserializeOwned,
{
    fn insert(
        &mut self,
        key: &T::Key,
        value: &T::SetValue,
    ) -> StorageResult<Option<T::GetValue>> {
        let key = key.database_key();
        Database::insert(self, key.as_ref(), T::column(), value).map_err(Into::into)
    }

    fn remove(&mut self, key: &T::Key) -> StorageResult<Option<T::GetValue>> {
        let key = key.database_key();
        Database::remove(self, key.as_ref(), T::column()).map_err(Into::into)
    }
}

// TODO: Implement this trait for all keys and use `type Type = MultiKey` for tuples.
//  -> After replace all common implementation with blanket, if possible.
/// Some keys requires pre-processing that could change their type.
pub trait IntoDatabaseKey {
    /// A new type of prepared database key that can be converted into bytes.
    type Type: AsRef<[u8]>;

    /// Coverts the key into database key that supports byte presentation.
    fn database_key(&self) -> Self::Type;
}

impl IntoDatabaseKey for BlockHeight {
    type Type = [u8; 4];

    fn database_key(&self) -> Self::Type {
        self.to_bytes()
    }
}
