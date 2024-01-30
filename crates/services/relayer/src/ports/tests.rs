use crate::{
    ports::RelayerDb,
    storage::{
        DaHeightTable,
        EventsHistory,
    },
};
use fuel_core_storage::test_helpers::MockStorage;
use fuel_core_types::entities::message::Message;
use std::borrow::Cow;
use test_case::test_case;

#[test]
fn test_insert_events() {
    let same_height = 12;
    let mut db = MockStorage::default();
    db.expect_insert::<EventsHistory>()
        .times(1)
        .returning(|_, _| Ok(None));
    db.expect_insert::<DaHeightTable>()
        .times(1)
        .withf(move |_, v| **v == same_height)
        .returning(|_, _| Ok(None));
    db.expect_commit().returning(|| Ok(()));
    db.expect_get::<DaHeightTable>()
        .once()
        .returning(|_| Ok(Some(std::borrow::Cow::Owned(9u64.into()))));
    let mut db = db.into_transactional();

    let mut m = Message::default();
    m.set_amount(10);
    m.set_da_height(same_height.into());
    let mut m2 = m.clone();
    m2.set_nonce(1.into());
    assert_ne!(m.id(), m2.id());
    let messages = [m.into(), m2.into()];
    db.insert_events(&same_height.into(), &messages[..])
        .unwrap();
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

    let mut db = MockStorage::default();
    db.expect_insert::<EventsHistory>()
        .returning(|_, _| Ok(None));
    db.expect_insert::<DaHeightTable>()
        .once()
        .withf(move |_, v| *v == same_height)
        .returning(|_, _| Ok(None));
    db.expect_commit().returning(|| Ok(()));
    db.expect_get::<DaHeightTable>()
        .once()
        .returning(|_| Ok(None));

    // When
    let mut db = db.into_transactional();
    let result = db.insert_events(&same_height, &events);

    // Then
    assert!(result.is_ok());
}

#[test]
fn insert_fails_for_messages_with_different_height() {
    // Given
    let last_height = 1u64;
    let events: Vec<_> = (0..=last_height)
        .map(|i| {
            let mut message = Message::default();
            message.set_da_height(i.into());
            message.set_amount(i);
            message.into()
        })
        .collect();

    let db = MockStorage::default();

    // When
    let mut db = db.into_transactional();
    let result = db.insert_events(&last_height.into(), &events);

    // Then
    let err = result.expect_err(
        "Should return error since DA message heights are different between each other",
    );
    assert!(err.to_string().contains("Invalid da height"));
}

#[test]
fn insert_fails_for_messages_same_height_but_on_different_height() {
    // Given
    let last_height = 1u64;
    let events: Vec<_> = (0..=last_height)
        .map(|i| {
            let mut message = Message::default();
            message.set_da_height(last_height.into());
            message.set_amount(i);
            message.into()
        })
        .collect();

    let db = MockStorage::default();

    // When
    let mut db = db.into_transactional();
    let next_height = last_height + 1;
    let result = db.insert_events(&next_height.into(), &events);

    // Then
    let err =
        result.expect_err("Should return error since DA message heights and commit da heights are different");
    assert!(err.to_string().contains("Invalid da height"));
}

#[test_case(None, 0, 0; "can set DA height to 0 when there is none available")]
#[test_case(None, 10, 10; "can set DA height to 10 when there is none available")]
#[test_case(0, 10, 10; "can set DA height to 10 when it is 0")]
#[test_case(0, None, 0; "inserts are bypassed when height goes from 0 to 0")]
#[test_case(10, 11, 11; "can set DA height to 11 when it is 10")]
#[test_case(11, None, 11; "inserts are bypassed when height goes from 11 to 11")]
#[test_case(11, None, 10; "inserts are bypassed when height reverted from 11 to 10")]
fn set_raises_da_height_monotonically(
    get: impl Into<Option<u64>>,
    inserts: impl Into<Option<u64>>,
    new_height: u64,
) {
    let mut db = MockStorage::default();
    if let Some(h) = inserts.into() {
        db.expect_insert::<DaHeightTable>()
            .once()
            .withf(move |_, v| **v == h)
            .returning(|_, _| Ok(None));
    }
    let get = get.into().map(|g| Cow::Owned(g.into()));
    db.expect_get::<DaHeightTable>()
        .once()
        .returning(move |_| Ok(get.clone()));
    db.expect_commit().returning(|| Ok(()));

    let mut db = db.into_transactional();
    db.set_finalized_da_height_to_at_least(&new_height.into())
        .unwrap();
}
