use std::{
    borrow::Cow,
    sync::{
        Arc,
        Mutex,
    },
};

use fuel_core_storage::{
    transactional::{
        StorageTransaction,
        Transaction,
    },
    StorageInspect,
};
use fuel_core_types::entities::message::Message;

use super::*;

#[test]
fn test_insert_messages() {
    let mut db = TestRelayerDb::default();
    db.message
        .lock()
        .unwrap()
        .expect_insert()
        .times(2)
        .returning(|_, _| Ok(None));
    db.relayer
        .lock()
        .unwrap()
        .expect_insert()
        .times(1)
        .withf(|_, v| **v == 12)
        .returning(|_, _| Ok(None));
    db.commit
        .lock()
        .unwrap()
        .expect_commit()
        .returning(|| Ok(()));

    let m = Message {
        amount: 10,
        da_height: 12u64.into(),
        ..Default::default()
    };
    let mut m2 = m.clone();
    m2.amount = 100;
    m2.da_height = 4u64.into();
    assert_ne!(m.id(), m2.id());
    let messages = [m.check(), m2.check()];
    db.insert_messages(&messages[..]).unwrap();
}

#[derive(Clone, Default)]
struct TestRelayerDb {
    message: Arc<Mutex<MockStorageMutate<Messages>>>,
    relayer: Arc<Mutex<MockStorageMutate<RelayerMetadata>>>,
    commit: Arc<Mutex<MockTransactional>>,
}

macro_rules! delegate {
    ($t:ty, $i:ident) => {
        impl StorageInspect<$t> for TestRelayerDb {
            type Error = fuel_core_storage::Error;

            fn get(
                &self,
                key: &<$t as Mappable>::Key,
            ) -> Result<Option<Cow<<$t as Mappable>::OwnedValue>>, Self::Error> {
                Ok(self
                    .$i
                    .lock()
                    .unwrap()
                    .get(key)?
                    .map(|i| Cow::Owned(i.into_owned())))
            }

            fn contains_key(
                &self,
                key: &<$t as Mappable>::Key,
            ) -> Result<bool, Self::Error> {
                self.$i.lock().unwrap().contains_key(key)
            }
        }
        impl StorageMutate<$t> for TestRelayerDb {
            fn insert(
                &mut self,
                key: &<$t as Mappable>::Key,
                value: &<$t as Mappable>::Value,
            ) -> Result<Option<<$t as Mappable>::OwnedValue>, Self::Error> {
                self.$i.lock().unwrap().insert(key, value)
            }

            fn remove(
                &mut self,
                key: &<$t as Mappable>::Key,
            ) -> Result<Option<<$t as Mappable>::OwnedValue>, Self::Error> {
                self.$i.lock().unwrap().remove(key)
            }
        }
    };
}
delegate!(Messages, message);
delegate!(RelayerMetadata, relayer);

impl Transactional for TestRelayerDb {
    type Storage = Self;
    fn transaction(
        &self,
    ) -> fuel_core_storage::transactional::StorageTransaction<Self::Storage> {
        StorageTransaction::new(self.clone())
    }
}

impl Transaction<TestRelayerDb> for TestRelayerDb {
    fn commit(&mut self) -> StorageResult<()> {
        self.commit.lock().unwrap().commit()
    }
}

impl AsMut<TestRelayerDb> for TestRelayerDb {
    fn as_mut(&mut self) -> &mut TestRelayerDb {
        self
    }
}
impl AsRef<TestRelayerDb> for TestRelayerDb {
    fn as_ref(&self) -> &TestRelayerDb {
        self
    }
}

mockall::mock! {
    StorageMutate<Type>
        where
        Type: Mappable,
        <Type as Mappable>::OwnedValue: 'static,
    {
    }
    impl<Type> StorageMutate<Type> for StorageMutate<Type>
        where
        Type: Mappable,
        <Type as Mappable>::OwnedValue: 'static,
    {
        fn insert(
            &mut self,
            key: &<Type as Mappable>::Key,
            value: &<Type as Mappable>::Value,
        ) -> Result<Option<<Type as Mappable>::OwnedValue>, <Self as StorageInspect<Type>>::Error>;

        fn remove(
            &mut self,
            key: &<Type as Mappable>::Key,
        ) -> Result<Option<<Type as Mappable>::OwnedValue>, <Self as StorageInspect<Type>>::Error>;
    }
    #[allow(clippy::complexity)]
    impl<Type> StorageInspect<Type> for StorageMutate<Type>
        where
        Type: Mappable,
        <Type as Mappable>::OwnedValue: 'static,
    {
        type Error = fuel_core_storage::Error;
        fn get(&self, key: &<Type as Mappable>::Key) -> Result<Option<Cow<'static, <Type as Mappable>::OwnedValue>>, <Self as StorageInspect<Type>>::Error>;
        fn contains_key(&self, key: &<Type as Mappable>::Key) -> Result<bool, <Self as StorageInspect<Type>>::Error>;
    }
}
mockall::mock! {
    Transactional{}
    impl Transaction<TestRelayerDb> for Transactional {
        fn commit(&mut self) -> StorageResult<()>;
    }
    impl AsRef<TestRelayerDb> for Transactional {
        fn as_ref(&self) -> &TestRelayerDb;
    }
    impl AsMut<TestRelayerDb> for Transactional {
        fn as_mut(&mut self) -> &mut TestRelayerDb;
    }

}
