#![allow(non_snake_case)]

use super::*;
use fuel_core_storage::{
    StorageMutate,
    transactional::WriteTransaction,
};
use rand::{
    Rng,
    SeedableRng,
    prelude::StdRng,
};
use std::sync::Arc;

#[tokio::test]
async fn get_receipt__gets_the_receipt_for_expected_tx() {
    let mut rng = StdRng::seed_from_u64(9999);

    // given
    let mut db = Database::in_memory();
    let tx_id = rng.r#gen();
    let expected = vec![Receipt::Return {
        id: rng.r#gen(),
        val: 987,
        pc: 123,
        is: 456,
    }];
    let status = TransactionExecutionStatus::Success {
        block_height: Default::default(),
        time: fuel_core_types::tai64::Tai64(123u64),
        result: None,
        receipts: Arc::new(expected.clone()),
        total_gas: 0,
        total_fee: 0,
    };
    let mut tx = db.write_transaction();
    StorageMutate::<TransactionStatuses>::insert(&mut tx, &tx_id, &status).unwrap();
    tx.commit().unwrap();
    let receipt_source = ReceiptSource::new(db);

    // when
    let actual = receipt_source.get_receipts(&tx_id).await.unwrap();

    // then
    assert_eq!(actual, expected);
}
