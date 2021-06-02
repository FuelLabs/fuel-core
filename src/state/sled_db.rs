use crate::state::{KeyValueStore, Transaction, TransactionResult};
use serde::de::DeserializeOwned;
use serde::Serialize;
use sled::transaction::{abort, ConflictableTransactionError, TransactionError, TransactionalTree};
use sled::{Error, IVec, Tree};
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

    fn get(&self, key: Self::Key) -> Option<Self::Value> {
        self.tree.get(key).unwrap().map(|v| bincode::deserialize(&v).unwrap())
    }

    fn put(&mut self, key: Self::Key, value: Self::Value) -> Option<Self::Value> {
        self.tree
            .insert(key, bincode::serialize(&value).unwrap())
            .unwrap()
            .map(|v| bincode::deserialize(&v).unwrap())
    }

    fn delete(&mut self, key: Self::Key) -> Option<Self::Value> {
        self.tree
            .remove(key)
            .unwrap()
            .map(|v| bincode::deserialize(&v).unwrap())
    }

    fn exists(&self, key: Self::Key) -> bool {
        self.tree.contains_key(key).unwrap()
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

    fn get(&self, key: Self::Key) -> Option<Self::Value> {
        self.inner.get(key).unwrap().map(|v| bincode::deserialize(&v).unwrap())
    }

    fn put(&mut self, key: Self::Key, value: Self::Value) -> Option<Self::Value> {
        let key_vec: Vec<u8> = key.into();
        self.inner
            .insert(key_vec, bincode::serialize(&value).unwrap())
            .unwrap()
            .map(|v| bincode::deserialize(&v).unwrap())
    }

    fn delete(&mut self, key: Self::Key) -> Option<Self::Value> {
        let key_vec: Vec<u8> = key.into();
        self.inner
            .remove(key_vec)
            .unwrap()
            .map(|v| bincode::deserialize(&v).unwrap())
    }

    fn exists(&self, key: Self::Key) -> bool {
        self.inner.get(key).unwrap().is_some()
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
            _ => ConflictableTransactionError::Abort(sled::Error::Unsupported("reverted".to_string())),
        }
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

            assert_eq!(sled_store.get("test".to_string()).unwrap(), "value".to_string());
        }

        #[test]
        fn can_insert_sled_db() {
            // setup
            let config = Config::new().temporary(true);
            let db = config.open().unwrap();
            let tree = db.open_tree(b"test_tree").unwrap();
            let mut sled_store: SledStore<String, String> = tree.into();

            // test
            let result = sled_store.put("test".to_string(), "value".to_string());

            // verify
            assert_eq!(result, None);
            assert_eq!(sled_store.get("test".to_string()).unwrap(), "value".to_string())
        }

        #[test]
        fn insert_returns_previous_value() {
            // setup
            let config = Config::new().temporary(true);
            let db = config.open().unwrap();
            let tree = db.open_tree(b"test_tree").unwrap();
            let mut sled_store: SledStore<String, String> = tree.into();

            // test
            let ret1 = sled_store.put("test".to_string(), "value".to_string());
            let ret2 = sled_store.put("test".to_string(), "other_value".to_string());

            // verify
            assert_eq!(ret1, None);
            assert_eq!(ret2, Some("value".to_string()));
            assert_eq!(sled_store.get("test".to_string()).unwrap(), "other_value".to_string())
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
            let value = sled_store.delete("test".to_string());

            // verify value was returned
            assert_eq!(value, Some("value".to_string()));
            // verify value is no longer available
            assert_eq!(sled_store.get("test".to_string()), None);
            // verify key no longer exists
            assert!(!sled_store.exists("test".to_string()))
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
                .transaction(|view| Ok(view.put("test".to_string(), "other_value".to_string())))
                .unwrap();

            assert_eq!(sled_store.get("test".to_string()).unwrap(), "other_value".to_string());
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
                let result = view.put("test".to_string(), "value".to_string());
                Err(Aborted)?;
                Ok(result)
            });

            assert_eq!(sled_store.get("test".to_string()), None);
            assert!(matches!(result, TransactionResult::Err(Aborted)));
        }
    }

    mod new_transactional_tree {
        use super::*;
        use sled::Config;

        #[test]
        fn can_get_from_tree() {
            todo!()
        }

        #[test]
        fn can_insert_into_tree() {
            todo!()
        }

        #[test]
        fn can_remove_from_tree() {
            todo!()
        }

        #[test]
        fn check_key_existence_from_tree() {
            todo!()
        }
    }
}
