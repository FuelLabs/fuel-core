use crate::state::Error::Codec;
use crate::state::{BatchOperations, KeyValueStore, Result, TransactableStorage, WriteOperation};
use serde::de::DeserializeOwned;
use serde::Serialize;
use sled::transaction::{ConflictableTransactionError, TransactionError};
use sled::{Error, Tree};
use std::fmt::Debug;
use std::marker::PhantomData;

#[derive(Clone, Debug)]
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

impl<K, V> KeyValueStore<K, V> for SledStore<K, V>
where
    K: AsRef<[u8]> + Debug + Clone,
    V: Debug + Serialize + DeserializeOwned + Clone,
{
    fn get(&self, key: &K) -> Result<Option<V>> {
        if let Some(value) = self.tree.get(key)? {
            Ok(Some(bincode::deserialize(&value).map_err(|_| Codec)?))
        } else {
            Ok(None)
        }
    }

    fn put(&mut self, key: K, value: V) -> Result<Option<V>> {
        let value = bincode::serialize(&value).map_err(|_| Codec)?;
        if let Some(previous_value) = self.tree.insert(key, value)? {
            Ok(Some(
                bincode::deserialize(&previous_value).map_err(|_| Codec)?,
            ))
        } else {
            Ok(None)
        }
    }

    fn delete(&mut self, key: &K) -> Result<Option<V>> {
        if let Some(previous_value) = self.tree.remove(key)? {
            Ok(Some(
                bincode::deserialize(&previous_value).map_err(|_| Codec)?,
            ))
        } else {
            Ok(None)
        }
    }

    fn exists(&self, key: &K) -> Result<bool> {
        Ok(self.tree.contains_key(key)?)
    }
}

impl<K, V> BatchOperations<K, V> for SledStore<K, V>
where
    K: AsRef<[u8]> + Debug + Clone + Send,
    V: Serialize + DeserializeOwned + Debug + Clone + Send,
{
    fn batch_write(
        &mut self,
        entries: &mut dyn Iterator<Item = WriteOperation<K, V>>,
    ) -> Result<()> {
        let mut batch = sled::Batch::default();
        for entry in entries {
            match entry {
                WriteOperation::Insert(key, value) => {
                    batch.insert(key.as_ref(), bincode::serialize(&value).map_err(|_| Codec)?);
                }
                WriteOperation::Remove(key) => {
                    batch.remove(key.as_ref());
                }
            }
        }
        self.tree.apply_batch(batch).map_err(Into::into)
    }
}

impl<K, V> TransactableStorage<K, V> for SledStore<K, V>
where
    K: AsRef<[u8]> + Debug + Clone + Send,
    V: Serialize + DeserializeOwned + Debug + Clone + Send,
{
}

impl From<sled::Error> for crate::state::Error {
    fn from(e: Error) -> Self {
        crate::state::Error::DatabaseError(Box::new(e))
    }
}

impl From<TransactionError> for crate::state::Error {
    fn from(e: TransactionError) -> Self {
        crate::state::Error::DatabaseError(Box::new(e))
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

#[cfg(test)]
mod tests {
    use super::*;

    mod sled_store {
        use super::*;
        use crate::state::Transaction;
        use crate::state::TransactionError::Aborted;
        use sled::Config;
        use std::sync::{Arc, Mutex};

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
                sled_store.get(&"test".to_string()).unwrap().unwrap(),
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
                sled_store.get(&"test".to_string()).unwrap().unwrap(),
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
                sled_store.get(&"test".to_string()).unwrap().unwrap(),
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
            let value = sled_store.delete(&"test".to_string()).unwrap();

            // verify value was returned
            assert_eq!(value, Some("value".to_string()));
            // verify value is no longer available
            assert_eq!(sled_store.get(&"test".to_string()).unwrap(), None);
            // verify key no longer exists
            assert!(!sled_store.exists(&"test".to_string()).unwrap())
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
            let mut sled_store: Arc<Mutex<SledStore<String, String>>> =
                Arc::new(Mutex::new(tree.into()));

            let result = sled_store
                .transaction(|view| {
                    let val = view
                        .put("test".to_string(), "other_value".to_string())
                        .unwrap();
                    Ok(val)
                })
                .unwrap();

            assert_eq!(
                sled_store
                    .lock()
                    .unwrap()
                    .get(&"test".to_string())
                    .unwrap()
                    .unwrap(),
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
            let mut sled_store: Arc<Mutex<SledStore<String, String>>> =
                Arc::new(Mutex::new(tree.into()));

            let result = sled_store.transaction(|view| {
                let result = view.put("test".to_string(), "value".to_string()).unwrap();
                Err(Aborted)?;
                Ok(result)
            });

            assert_eq!(
                sled_store.lock().unwrap().get(&"test".to_string()).unwrap(),
                None
            );
            assert!(matches!(
                result,
                Err(crate::state::TransactionError::Aborted)
            ));
        }
    }
}
