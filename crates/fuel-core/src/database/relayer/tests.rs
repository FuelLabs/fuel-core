use super::*;

#[test]
fn test_insert_messages() {
    let mut db = Database::default();

    let m = Message {
        amount: 10,
        ..Default::default()
    };
    let mut m2 = m.clone();
    m2.amount = 100;
    assert_ne!(m.id(), m2.id());
    let messages = [(12u64.into(), m.check()), (4u64.into(), m2.check())];
    db.insert_messages(&messages[..]).unwrap();

    let h: DaBlockHeight = db
        .get(metadata::FINALIZED_DA_HEIGHT_KEY, Column::Metadata)
        .unwrap()
        .unwrap();
    assert_eq!(*h, 12);
    let result: Vec<(Vec<u8>, Message)> = db
        .iter_all(Column::RelayerMessages, None, None, None)
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(&result[0].1, messages[1].1.message());
    assert_eq!(&result[1].1, messages[0].1.message());
}
