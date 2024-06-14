use crate::{
    ports::{
        DatabaseTransaction,
        MockDatabaseTransaction,
        RelayerDb,
        Transactional,
    },
    storage::EventsHistory,
    Config,
};
use fuel_core_storage::test_helpers::{
    MockBasic,
    MockStorage,
};
use fuel_core_types::{
    blockchain::primitives::DaBlockHeight,
    entities::{
        Message,
        RelayedTransaction,
    },
    services::relayer::Event,
};

type DBTx = MockStorage<MockBasic, MockDatabaseTransaction>;
type ReturnDB = Box<dyn Fn() -> DBTx + Send + Sync>;

impl DatabaseTransaction for DBTx {
    fn commit(self) -> fuel_core_storage::Result<()> {
        self.data.commit()
    }
}

type MockDatabase = MockStorage<MockBasic, ReturnDB>;

impl Transactional for MockDatabase {
    type Transaction<'a> = DBTx;

    fn transaction(&mut self) -> Self::Transaction<'_> {
        (self.data)()
    }

    fn latest_da_height(&self) -> Option<DaBlockHeight> {
        Some(Config::DEFAULT_DA_DEPLOY_HEIGHT.into())
    }
}

#[test]
fn test_insert_events() {
    // Given
    let same_height = 12u64;
    let return_db_tx = move || {
        let mut db = DBTx::default();
        db.storage
            .expect_insert::<EventsHistory>()
            .times(1)
            .returning(|_, _| Ok(None));
        db.data.expect_commit().returning(|| Ok(()));
        db
    };

    let mut db = MockDatabase {
        data: Box::new(return_db_tx),
        storage: Default::default(),
    };

    let mut m = Message::default();
    m.set_amount(10);
    m.set_da_height(same_height.into());

    let mut m2 = m.clone();
    m2.set_nonce(1.into());
    assert_ne!(m.id(), m2.id());

    let messages = [m.into(), m2.into()];

    // When
    let result = db.insert_events(&same_height.into(), &messages[..]);

    // Then
    assert!(result.is_ok());
}

#[test]
fn insert_always_raises_da_height_monotonically() {
    // Given
    let same_height = 12u64.into();
    let events: Vec<_> = (0..10)
        .map(|i| {
            let mut message = Message::default();
            message.set_amount(i);
            message.set_da_height(same_height);
            message
        })
        .map(Into::into)
        .collect();

    let return_db_tx = move || {
        let mut db = DBTx::default();
        db.storage
            .expect_insert::<EventsHistory>()
            .returning(|_, _| Ok(None));
        db.data.expect_commit().returning(|| Ok(()));
        db
    };

    let mut db = MockDatabase {
        data: Box::new(return_db_tx),
        storage: Default::default(),
    };

    // When
    let result = db.insert_events(&same_height, &events);

    // Then
    assert!(result.is_ok());
}

#[test]
fn insert_fails_for_events_with_different_height() {
    fn inner_test<F: Fn(u64) -> Event>(f: F) {
        // Given
        let last_height = 1u64;
        let events: Vec<_> = (0..=last_height).map(f).collect();

        let mut db = MockDatabase {
            data: Box::new(DBTx::default),
            storage: Default::default(),
        };

        // When
        let result = db.insert_events(&last_height.into(), &events);

        // Then
        let err = result.expect_err(
            "Should return error since DA message heights are different between each other",
        );
        assert!(err.to_string().contains("Invalid da height"));
    }

    // test with messages
    inner_test(|i| {
        let mut message = Message::default();
        message.set_da_height(i.into());
        message.set_amount(i);
        message.into()
    });

    // test with forced transactions
    inner_test(|i| {
        let mut transaction = RelayedTransaction::default();
        transaction.set_nonce(i.into());
        transaction.set_da_height(i.into());
        transaction.set_max_gas(i);
        transaction.into()
    })
}

#[test]
fn insert_fails_for_events_same_height_but_on_different_height() {
    fn inner_test<F: Fn(u64) -> Event>(f: F, last_height: u64) {
        // Given
        let events: Vec<_> = (0..=last_height).map(f).collect();

        // When
        let mut db = MockDatabase {
            data: Box::new(DBTx::default),
            storage: Default::default(),
        };

        let next_height = last_height + 1;
        let result = db.insert_events(&next_height.into(), &events);

        // Then
        let err =
            result.expect_err("Should return error since DA message heights and commit da heights are different");
        assert!(err.to_string().contains("Invalid da height"));
    }

    let last_height = 1u64;
    // messages
    inner_test(
        |i| {
            let mut message = Message::default();
            message.set_da_height(last_height.into());
            message.set_amount(i);
            message.into()
        },
        last_height,
    );
    // relayed transactions
    inner_test(
        |i| {
            let mut transaction = RelayedTransaction::default();
            transaction.set_nonce(i.into());
            transaction.set_da_height(last_height.into());
            transaction.set_max_gas(i);
            transaction.into()
        },
        last_height,
    );
}
