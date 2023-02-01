use fuel_core_storage::test_helpers::MockStorage;
use fuel_core_types::entities::message::Message;

use super::*;

#[test]
fn test_insert_messages() {
    let mut db = MockStorage::default();
    db.expect_insert::<Messages>()
        .times(2)
        .returning(|_, _| Ok(None));
    db.expect_insert::<RelayerMetadata>()
        .times(1)
        .withf(|_, v| **v == 12)
        .returning(|_, _| Ok(None));
    db.expect_commit().returning(|| Ok(()));
    let mut db = db.into_transactional();

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
