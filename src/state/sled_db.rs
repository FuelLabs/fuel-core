use crate::state::Error::Codec;
use crate::state::{KeyValueStore, Result, Transaction, TransactionResult};
use serde::de::DeserializeOwned;
use serde::Serialize;
use sled::transaction::{
    ConflictableTransactionError, TransactionError, TransactionalTree, UnabortableTransactionError,
};
use sled::{Error, Tree};
use std::fmt::Debug;
use std::marker::PhantomData;

pub struct SledStore<K, V> {
    tree: Tree,
    _key: PhantomData<K>,
    _value: PhantomData<V>,
}

impl<K, V> From<Tree> for SledStore<K, V> {
    fn from(tree: Tree) -> Self {
        SledStore {
            tree,
            _key: PhantomData,
            _value: PhantomData,
        }
    }
}

impl<K, V> KeyValueStore for SledStore<K, V>
where
    K: AsRef<[u8]> + Debug + Clone,
    V: Debug + Serialize + DeserializeOwned + Clone,
{
    type Key = K;
    type Value = V;

    fn get(&self, key: Self::Key) -> Result<Option<Self::Value>> {
        if let Some(value) = self.tree.get(key)? {
            Ok(Some(bincode::deserialize(&value).map_err(|_| Codec)?))
        } else {
            Ok(None)
        }
    }

    fn put(&mut self, key: Self::Key, value: Self::Value) -> Result<Option<Self::Value>> {
        let value = bincode::serialize(&value).map_err(|_| Codec)?;
        if let Some(previous_value) = self.tree.insert(key, value)? {
            Ok(Some(
                bincode::deserialize(&previous_value).map_err(|_| Codec)?,
            ))
        } else {
            Ok(None)
        }
    }

    fn delete(&mut self, key: Self::Key) -> Result<Option<Self::Value>> {
        if let Some(previous_value) = self.tree.remove(key)? {
            Ok(Some(
                bincode::deserialize(&previous_value).map_err(|_| Codec)?,
            ))
        } else {
            Ok(None)
        }
    }

    fn exists(&self, key: Self::Key) -> Result<bool> {
        Ok(self.tree.contains_key(key)?)
    }
}

impl<K, V> Transaction<K, V, NewTransactionalTree<K, V>> for SledStore<K, V>
where
    K: AsRef<[u8]> + Into<Vec<u8>> + Debug + Clone,
    V: Debug + Serialize + DeserializeOwned + Clone,
{
    fn transaction<F, R>(&mut self, f: F) -> TransactionResult<R>
    where
        F: FnOnce(&mut NewTransactionalTree<K, V>) -> TransactionResult<R> + Copy,
    {
        self.tree
            .transaction(|tree| {
                let tree = tree.clone();
                let mut new_tree = NewTransactionalTree {
                    inner: tree,
                    _key: PhantomData,
                    _value: PhantomData,
                };
                let result = f(&mut new_tree)?;
                Ok(result)
            })
            .map_err(Into::into)
    }
}

struct NewTransactionalTree<K, V> {
    inner: TransactionalTree,
    _key: PhantomData<K>,
    _value: PhantomData<V>,
}

impl<K, V> KeyValueStore for NewTransactionalTree<K, V>
where
    K: AsRef<[u8]> + Into<Vec<u8>> + Debug + Clone,
    V: Debug + Serialize + DeserializeOwned + Clone,
{
    type Key = K;
    type Value = V;

    fn get(&self, key: Self::Key) -> Result<Option<Self::Value>> {
        if let Some(value) = self.inner.get(key)? {
            Ok(Some(bincode::deserialize(&value).map_err(|_| Codec)?))
        } else {
            Ok(None)
        }
    }

    fn put(&mut self, key: Self::Key, value: Self::Value) -> Result<Option<Self::Value>> {
        let key_vec: Vec<u8> = key.into();
        let value = bincode::serialize(&value).map_err(|_| Codec)?;
        if let Some(previous_value) = self.inner.insert(key_vec, value)? {
            Ok(Some(
                bincode::deserialize(previous_value.as_ref()).map_err(|_| Codec)?,
            ))
        } else {
            Ok(None)
        }
    }

    fn delete(&mut self, key: Self::Key) -> Result<Option<Self::Value>> {
        let key_vec: Vec<u8> = key.into();
        if let Some(previous_value) = self.inner.remove(key_vec)? {
            Ok(Some(
                bincode::deserialize(&previous_value).map_err(|_| Codec)?,
            ))
        } else {
            Ok(None)
        }
    }

    fn exists(&self, key: Self::Key) -> Result<bool> {
        Ok(self.inner.get(key)?.is_some())
    }
}

impl From<sled::Error> for crate::state::Error {
    fn from(e: Error) -> Self {
        crate::state::Error::DatabaseError(Box::new(e))
    }
}

impl From<TransactionError> for crate::state::TransactionError {
    fn from(e: TransactionError) -> Self {
        match e {
            TransactionError::Abort(_) => crate::state::TransactionError::Aborted,
            TransactionError::Storage(_) => crate::state::TransactionError::Aborted,
        }
    }
}

impl From<crate::state::TransactionError> for ConflictableTransactionError {
    fn from(e: crate::state::TransactionError) -> Self {
        match e {
            _ => ConflictableTransactionError::Abort(sled::Error::Unsupported(
                "reverted".to_string(),
            )),
        }
    }
}

impl From<UnabortableTransactionError> for crate::state::Error {
    fn from(e: UnabortableTransactionError) -> Self {
        crate::state::Error::DatabaseError(Box::new(e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod sled_store {
        use super::*;
        use crate::state::Transaction;
        use crate::state::TransactionError::Aborted;
        use sled::Config;

        #[test]
        fn can_get_from_sled_store() {
            let config = Config::new().temporary(true);
            let db = config.open().unwrap();
            let tree = db.open_tree(b"test_tree").unwrap();

            let v = bincode::serialize("value").unwrap();
            tree.insert("test".to_string(), v.clone()).unwrap();
            // convert from sled tree to our storage interface
            let sled_store: SledStore<String, String> = tree.into();

            assert_eq!(
                sled_store.get("test".to_string()).unwrap().unwrap(),
                "value".to_string()
            );
        }

        #[test]
        fn can_insert_sled_db() {
            // setup
            let config = Config::new().temporary(true);
            let db = config.open().unwrap();
            let tree = db.open_tree(b"test_tree").unwrap();
            let mut sled_store: SledStore<String, String> = tree.into();

            // test
            let result = sled_store
                .put("test".to_string(), "value".to_string())
                .unwrap();

            // verify
            assert_eq!(result, None);
            assert_eq!(
                sled_store.get("test".to_string()).unwrap().unwrap(),
                "value".to_string()
            )
        }

        #[test]
        fn insert_returns_previous_value() {
            // setup
            let config = Config::new().temporary(true);
            let db = config.open().unwrap();
            let tree = db.open_tree(b"test_tree").unwrap();
            let mut sled_store: SledStore<String, String> = tree.into();

            // test
            let ret1 = sled_store
                .put("test".to_string(), "value".to_string())
                .unwrap();
            let ret2 = sled_store
                .put("test".to_string(), "other_value".to_string())
                .unwrap();

            // verify
            assert_eq!(ret1, None);
            assert_eq!(ret2, Some("value".to_string()));
            assert_eq!(
                sled_store.get("test".to_string()).unwrap().unwrap(),
                "other_value".to_string()
            )
        }

        #[test]
        fn can_remove_values() {
            // setup
            let config = Config::new().temporary(true);
            let db = config.open().unwrap();
            let tree = db.open_tree(b"test_tree").unwrap();
            let v = bincode::serialize("value").unwrap();
            tree.insert("test".to_string(), v.clone()).unwrap();
            let mut sled_store: SledStore<String, String> = tree.into();
            // test
            let value = sled_store.delete("test".to_string()).unwrap();

            // verify value was returned
            assert_eq!(value, Some("value".to_string()));
            // verify value is no longer available
            assert_eq!(sled_store.get("test".to_string()).unwrap(), None);
            // verify key no longer exists
            assert!(!sled_store.exists("test".to_string()).unwrap())
        }

        #[test]
        fn can_execute_transaction() {
            // setup
            let config = Config::new().temporary(true);
            let db = config.open().unwrap();
            let tree = db.open_tree(b"test_tree").unwrap();

            let v = bincode::serialize("value").unwrap();
            tree.insert("test".to_string(), v.clone()).unwrap();
            // convert from sled tree to our storage interface
            let mut sled_store: SledStore<String, String> = tree.into();

            let result = sled_store
                .transaction(|view| {
                    Ok(view
                        .put("test".to_string(), "other_value".to_string())
                        .unwrap())
                })
                .unwrap();

            assert_eq!(
                sled_store.get("test".to_string()).unwrap().unwrap(),
                "other_value".to_string()
            );
            assert_eq!(result, Some("value".to_string()));
        }

        #[test]
        fn aborted_transaction_reverts_any_changes() {
            // setup
            let config = Config::new().temporary(true);
            let db = config.open().unwrap();
            let tree = db.open_tree(b"test_tree").unwrap();
            let mut sled_store: SledStore<String, String> = tree.into();

            let result = sled_store.transaction(|view| {
                let result = view.put("test".to_string(), "value".to_string()).unwrap();
                Err(Aborted)?;
                Ok(result)
            });

            assert_eq!(sled_store.get("test".to_string()).unwrap(), None);
            assert!(matches!(result, TransactionResult::Err(Aborted)));
        }
    }

    mod new_transactional_tree {

        use super::*;
        use sled::Config;

        #[test]
        fn can_get_from_tree() {
            // setup
            let config = Config::new().temporary(true);
            let db = config.open().unwrap();
            let tree = db.open_tree(b"test_tree").unwrap();

            let v = bincode::serialize("value").unwrap();
            tree.insert("test".to_string(), v.clone()).unwrap();
            // convert from sled tree to our storage interface
            let mut sled_store: SledStore<String, String> = tree.into();

            let result = sled_store
                .transaction(|view| Ok(view.get("test".to_string()).unwrap()))
                .unwrap();

            assert_eq!(result, Some("value".to_string()));
        }

        #[test]
        fn can_insert_into_tree() {
            // setup
            let config = Config::new().temporary(true);
            let db = config.open().unwrap();
            let tree = db.open_tree(b"test_tree").unwrap();
            let mut sled_store: SledStore<String, String> = tree.into();

            let result = sled_store
                .transaction(|view| Ok(view.put("test".to_string(), "value".to_string()).unwrap()))
                .unwrap();

            assert_eq!(result, None);
            assert_eq!(
                sled_store.get("test".to_string()).unwrap(),
                Some("value".to_string())
            );
        }

        #[test]
        fn can_remove_from_tree() {
            // setup
            let config = Config::new().temporary(true);
            let db = config.open().unwrap();
            let tree = db.open_tree(b"test_tree").unwrap();

            let v = bincode::serialize("value").unwrap();
            tree.insert("test".to_string(), v.clone()).unwrap();
            // convert from sled tree to our storage interface
            let mut sled_store: SledStore<String, String> = tree.into();

            let result = sled_store
                .transaction(|view| Ok(view.delete("test".to_string()).unwrap()))
                .unwrap();

            assert_eq!(result, Some("value".to_string()));
            assert_eq!(sled_store.get("test".to_string()).unwrap(), None);
        }

        #[test]
        fn check_key_existence_from_tree() {
            // setup
            let config = Config::new().temporary(true);
            let db = config.open().unwrap();
            let tree = db.open_tree(b"test_tree").unwrap();

            let v = bincode::serialize("value").unwrap();
            tree.insert("test".to_string(), v.clone()).unwrap();
            // convert from sled tree to our storage interface
            let mut sled_store: SledStore<String, String> = tree.into();

            let result = sled_store
                .transaction(|view| Ok(view.exists("test".to_string()).unwrap()))
                .unwrap();

            assert!(result);
        }
    }
}
