use crate::{
    ports::TxPoolDb,
    test_helpers::{
        add_coin_to_state,
        create_output_and_input,
        custom_predicate,
        random_predicate,
        setup_coin,
        IntoEstimated,
        TEST_COIN_AMOUNT,
    },
    txpool::test_helpers::{
        create_coin_output,
        create_contract_input,
        create_contract_output,
        create_message_predicate_from_message,
    },
    Config,
    Error,
    MockDb,
    TxPool,
};
use fuel_core_types::{
    fuel_asm::{
        op,
        RegId,
        Word,
    },
    fuel_crypto::rand::{
        rngs::StdRng,
        SeedableRng,
    },
    fuel_tx,
    fuel_tx::{
        input::coin::CoinPredicate,
        Address,
        AssetId,
        Contract,
        Input,
        Output,
        Transaction,
        TransactionBuilder,
        UniqueIdentifier,
        UtxoId,
    },
    fuel_vm::checked_transaction::Checked,
};

use std::{
    cmp::Reverse,
    collections::HashMap,
    vec,
};

use super::check_single_tx;

const GAS_LIMIT: Word = 1000;

async fn check_unwrap_tx(
    tx: Transaction,
    db: MockDb,
    config: &Config,
) -> Checked<Transaction> {
    check_single_tx(tx, db.current_block_height().unwrap(), config)
        .await
        .expect("Transaction should be checked")
}

async fn check_tx(
    tx: Transaction,
    db: MockDb,
    config: &Config,
) -> anyhow::Result<Checked<Transaction>> {
    check_single_tx(tx, db.current_block_height().unwrap(), config).await
}

#[tokio::test]
async fn insert_simple_tx_succeeds() {
    let mut rng = StdRng::seed_from_u64(0);
    let db = MockDb::default();
    let mut txpool = TxPool::new(Default::default(), db.clone());

    let (_, gas_coin) = setup_coin(&mut rng, Some(&txpool.database));
    let tx = TransactionBuilder::script(vec![], vec![])
        .gas_limit(GAS_LIMIT)
        .add_input(gas_coin)
        .finalize_as_transaction();

    let tx = check_unwrap_tx(tx, db.clone(), &txpool.config).await;

    txpool
        .insert_inner(tx)
        .expect("Transaction should be OK, got Err");
}

#[tokio::test]
async fn insert_simple_tx_dependency_chain_succeeds() {
    let mut rng = StdRng::seed_from_u64(0);
    let db = MockDb::default();
    let mut txpool = TxPool::new(Default::default(), db.clone());

    let (_, gas_coin) = setup_coin(&mut rng, Some(&txpool.database));
    let (output, unset_input) = create_output_and_input(&mut rng, 1);
    let tx1 = TransactionBuilder::script(vec![], vec![])
        .gas_price(1)
        .gas_limit(GAS_LIMIT)
        .add_input(gas_coin)
        .add_output(output)
        .finalize_as_transaction();

    let (_, gas_coin) = setup_coin(&mut rng, Some(&txpool.database));
    let input = unset_input.into_input(UtxoId::new(
        tx1.id(&fuel_tx::ConsensusParameters::DEFAULT.chain_id),
        0,
    ));
    let tx2 = TransactionBuilder::script(vec![], vec![])
        .gas_price(1)
        .gas_limit(GAS_LIMIT)
        .add_input(input)
        .add_input(gas_coin)
        .finalize_as_transaction();

    let tx1 = check_unwrap_tx(tx1, db.clone(), &txpool.config).await;
    let tx2 = check_unwrap_tx(tx2, db.clone(), &txpool.config).await;

    txpool.insert_inner(tx1).expect("Tx1 should be OK, got Err");
    txpool
        .insert_inner(tx2)
        .expect("Tx2 dependent should be OK, got Err");
}

#[tokio::test]
async fn faulty_t2_collided_on_contract_id_from_tx1() {
    let mut rng = StdRng::seed_from_u64(0);
    let db = MockDb::default();
    let mut txpool = TxPool::new(Default::default(), db.clone());

    let contract_id = Contract::EMPTY_CONTRACT_ID;

    // contract creation tx
    let (_, gas_coin) = setup_coin(&mut rng, Some(&txpool.database));
    let (output, unset_input) = create_output_and_input(&mut rng, 10);
    let tx = TransactionBuilder::create(
        Default::default(),
        Default::default(),
        Default::default(),
    )
    .gas_price(10)
    .gas_limit(GAS_LIMIT)
    .add_input(gas_coin)
    .add_output(create_contract_output(contract_id))
    .add_output(output)
    .finalize_as_transaction();

    let (_, gas_coin) = setup_coin(&mut rng, Some(&txpool.database));
    let input = unset_input.into_input(UtxoId::new(
        tx.id(&fuel_tx::ConsensusParameters::DEFAULT.chain_id),
        1,
    ));

    // attempt to insert a different creation tx with a valid dependency on the first tx,
    // but with a conflicting output contract id
    let tx_faulty = TransactionBuilder::create(
        Default::default(),
        Default::default(),
        Default::default(),
    )
    .gas_price(9)
    .gas_limit(GAS_LIMIT)
    .add_input(gas_coin)
    .add_input(input)
    .add_output(create_contract_output(contract_id))
    .add_output(output)
    .finalize_as_transaction();

    let tx = check_unwrap_tx(tx, db.clone(), &txpool.config).await;
    txpool.insert_inner(tx).expect("Tx1 should be Ok, got Err");

    let tx_faulty = check_unwrap_tx(tx_faulty, db.clone(), &txpool.config).await;

    let err = txpool
        .insert_inner(tx_faulty)
        .expect_err("Tx2 should be Err, got Ok");
    assert!(matches!(
        err.downcast_ref::<Error>(),
        Some(Error::NotInsertedCollisionContractId(id)) if id == &contract_id
    ));
}

#[tokio::test]
async fn fail_to_insert_tx_with_dependency_on_invalid_utxo_type() {
    let mut rng = StdRng::seed_from_u64(0);
    let db = MockDb::default();
    let mut txpool = TxPool::new(Default::default(), db.clone());

    let contract_id = Contract::EMPTY_CONTRACT_ID;
    let (_, gas_coin) = setup_coin(&mut rng, Some(&txpool.database));
    let tx_faulty = TransactionBuilder::create(
        Default::default(),
        Default::default(),
        Default::default(),
    )
    .add_input(gas_coin)
    .add_output(create_contract_output(contract_id))
    .gas_limit(GAS_LIMIT)
    .finalize_as_transaction();

    // create a second transaction with utxo id referring to
    // the wrong type of utxo (contract instead of coin)
    let tx = TransactionBuilder::script(vec![], vec![])
        .gas_price(1)
        .gas_limit(GAS_LIMIT)
        .add_input(random_predicate(
            &mut rng,
            AssetId::BASE,
            TEST_COIN_AMOUNT,
            Some(UtxoId::new(
                tx_faulty.id(&fuel_tx::ConsensusParameters::DEFAULT.chain_id),
                0,
            )),
        ))
        .finalize_as_transaction();

    let tx_faulty_id = tx_faulty.id(&fuel_tx::ConsensusParameters::DEFAULT.chain_id);
    let tx_faulty = check_unwrap_tx(tx_faulty, db.clone(), &txpool.config).await;

    txpool
        .insert_inner(tx_faulty.clone())
        .expect("Tx1 should be Ok, got Err");

    let tx = check_unwrap_tx(tx, db.clone(), &txpool.config).await;

    let err = txpool
        .insert_inner(tx)
        .expect_err("Tx2 should be Err, got Ok");
    assert!(matches!(
        err.downcast_ref::<Error>(),
        Some(Error::NotInsertedInputUtxoIdNotExisting(id)) if id == &UtxoId::new(tx_faulty_id, 0)
    ));
}

#[tokio::test]
async fn not_inserted_known_tx() {
    let config = Config {
        utxo_validation: false,
        ..Default::default()
    };
    let db = MockDb::default();
    let mut txpool = TxPool::new(config, db.clone());

    let tx = Transaction::default_test_tx();
    let tx = check_unwrap_tx(tx, db.clone(), &txpool.config).await;

    txpool
        .insert_inner(tx.clone())
        .expect("Tx1 should be Ok, got Err");

    let err = txpool
        .insert_inner(tx)
        .expect_err("Second insertion of Tx1 should be Err, got Ok");
    assert!(matches!(
        err.downcast_ref::<Error>(),
        Some(Error::NotInsertedTxKnown)
    ));
}

#[tokio::test]
async fn try_to_insert_tx2_missing_utxo() {
    let mut rng = StdRng::seed_from_u64(0);
    let mut txpool = TxPool::new(Default::default(), MockDb::default());

    let (_, input) = setup_coin(&mut rng, None);
    let tx = TransactionBuilder::script(vec![], vec![])
        .gas_price(10)
        .gas_limit(GAS_LIMIT)
        .add_input(input)
        .finalize_as_transaction();

    let tx = check_unwrap_tx(tx, txpool.database.clone(), &txpool.config).await;

    let err = txpool
        .insert_inner(tx)
        .expect_err("Tx should be Err, got Ok");
    assert!(matches!(
        err.downcast_ref::<Error>(),
        Some(Error::NotInsertedInputUtxoIdNotExisting(_))
    ));
}

#[tokio::test]
async fn higher_priced_tx_removes_lower_priced_tx() {
    let mut rng = StdRng::seed_from_u64(0);
    let db = MockDb::default();
    let mut txpool = TxPool::new(Default::default(), db.clone());

    let (_, coin_input) = setup_coin(&mut rng, Some(&txpool.database));

    let tx1 = TransactionBuilder::script(vec![], vec![])
        .gas_price(10)
        .gas_limit(GAS_LIMIT)
        .add_input(coin_input.clone())
        .finalize_as_transaction();

    let tx2 = TransactionBuilder::script(vec![], vec![])
        .gas_price(20)
        .gas_limit(GAS_LIMIT)
        .add_input(coin_input)
        .finalize_as_transaction();

    let tx1_id = tx1.id(&fuel_tx::ConsensusParameters::DEFAULT.chain_id);
    let tx1 = check_unwrap_tx(tx1, db.clone(), &txpool.config).await;

    txpool
        .insert_inner(tx1.clone())
        .expect("Tx1 should be Ok, got Err");

    let tx2 = check_unwrap_tx(tx2, db.clone(), &txpool.config).await;

    let vec = txpool.insert_inner(tx2).expect("Tx2 should be Ok, got Err");
    assert_eq!(vec.removed[0].id(), tx1_id, "Tx1 id should be removed");
}

#[tokio::test]
async fn underpriced_tx1_not_included_coin_collision() {
    let mut rng = StdRng::seed_from_u64(0);
    let db = MockDb::default();
    let mut txpool = TxPool::new(Default::default(), db.clone());

    let (_, gas_coin) = setup_coin(&mut rng, Some(&txpool.database));
    let (output, unset_input) = create_output_and_input(&mut rng, 10);
    let tx1 = TransactionBuilder::script(vec![], vec![])
        .gas_price(20)
        .gas_limit(GAS_LIMIT)
        .add_input(gas_coin)
        .add_output(output)
        .finalize_as_transaction();

    let input = unset_input.into_input(UtxoId::new(
        tx1.id(&fuel_tx::ConsensusParameters::DEFAULT.chain_id),
        0,
    ));

    let tx2 = TransactionBuilder::script(vec![], vec![])
        .gas_price(20)
        .gas_limit(GAS_LIMIT)
        .add_input(input.clone())
        .finalize_as_transaction();

    let tx3 = TransactionBuilder::script(vec![], vec![])
        .gas_price(10)
        .gas_limit(GAS_LIMIT)
        .add_input(input)
        .finalize_as_transaction();

    let tx1_checked = check_unwrap_tx(tx1.clone(), db.clone(), txpool.config()).await;
    txpool
        .insert_inner(tx1_checked)
        .expect("Tx1 should be Ok, got Err");

    let tx2_checked = check_unwrap_tx(tx2.clone(), db.clone(), txpool.config()).await;
    txpool
        .insert_inner(tx2_checked)
        .expect("Tx2 should be Ok, got Err");

    let tx3_checked = check_unwrap_tx(tx3, db.clone(), txpool.config()).await;
    let err = txpool
        .insert_inner(tx3_checked)
        .expect_err("Tx3 should be Err, got Ok");
    assert!(matches!(
        err.downcast_ref::<Error>(),
        Some(Error::NotInsertedCollision(id, utxo_id)) if id == &tx2.id(&fuel_tx::ConsensusParameters::DEFAULT.chain_id) && utxo_id == &UtxoId::new(tx1.id(&fuel_tx::ConsensusParameters::DEFAULT.chain_id), 0)
    ));
}

#[tokio::test]
async fn overpriced_tx_contract_input_not_inserted() {
    let mut rng = StdRng::seed_from_u64(0);
    let db = MockDb::default();
    let mut txpool = TxPool::new(Default::default(), db.clone());

    let contract_id = Contract::EMPTY_CONTRACT_ID;
    let (_, gas_funds) = setup_coin(&mut rng, Some(&txpool.database));
    let tx1 = TransactionBuilder::create(
        Default::default(),
        Default::default(),
        Default::default(),
    )
    .gas_price(10)
    .gas_limit(GAS_LIMIT)
    .add_input(gas_funds)
    .add_output(create_contract_output(contract_id))
    .finalize_as_transaction();

    let (_, gas_funds) = setup_coin(&mut rng, Some(&txpool.database));
    let tx2 = TransactionBuilder::script(vec![], vec![])
        .gas_price(11)
        .gas_limit(GAS_LIMIT)
        .add_input(gas_funds)
        .add_input(create_contract_input(
            Default::default(),
            Default::default(),
            contract_id,
        ))
        .add_output(Output::contract(1, Default::default(), Default::default()))
        .finalize_as_transaction();

    let tx1 = check_unwrap_tx(tx1, db.clone(), &txpool.config).await;
    txpool.insert_inner(tx1).expect("Tx1 should be Ok, got err");

    let tx2 = check_unwrap_tx(tx2, db.clone(), &txpool.config).await;
    let err = txpool
        .insert_inner(tx2)
        .expect_err("Tx2 should be Err, got Ok");
    assert!(
        matches!(
            err.downcast_ref::<Error>(),
            Some(Error::NotInsertedContractPricedLower(id)) if id == &contract_id
        ),
        "wrong err {err:?}"
    );
}

#[tokio::test]
async fn dependent_contract_input_inserted() {
    let mut rng = StdRng::seed_from_u64(0);
    let db = MockDb::default();
    let mut txpool = TxPool::new(Default::default(), db.clone());

    let contract_id = Contract::EMPTY_CONTRACT_ID;
    let (_, gas_funds) = setup_coin(&mut rng, Some(&txpool.database));
    let tx1 = TransactionBuilder::create(
        Default::default(),
        Default::default(),
        Default::default(),
    )
    .gas_price(10)
    .gas_limit(GAS_LIMIT)
    .add_input(gas_funds)
    .add_output(create_contract_output(contract_id))
    .finalize_as_transaction();

    let (_, gas_funds) = setup_coin(&mut rng, Some(&txpool.database));
    let tx2 = TransactionBuilder::script(vec![], vec![])
        .gas_price(10)
        .gas_limit(GAS_LIMIT)
        .add_input(gas_funds)
        .add_input(create_contract_input(
            Default::default(),
            Default::default(),
            contract_id,
        ))
        .add_output(Output::contract(1, Default::default(), Default::default()))
        .finalize_as_transaction();

    let tx1 = check_unwrap_tx(tx1, db.clone(), &txpool.config).await;
    let tx2 = check_unwrap_tx(tx2, db.clone(), &txpool.config).await;
    txpool.insert_inner(tx1).expect("Tx1 should be Ok, got Err");
    txpool.insert_inner(tx2).expect("Tx2 should be Ok, got Err");
}

#[tokio::test]
async fn more_priced_tx3_removes_tx1_and_dependent_tx2() {
    let mut rng = StdRng::seed_from_u64(0);
    let db = MockDb::default();
    let mut txpool = TxPool::new(Default::default(), db.clone());

    let (_, gas_coin) = setup_coin(&mut rng, Some(&txpool.database));

    let (output, unset_input) = create_output_and_input(&mut rng, 10);
    let tx1 = TransactionBuilder::script(vec![], vec![])
        .gas_price(10)
        .gas_limit(GAS_LIMIT)
        .add_input(gas_coin.clone())
        .add_output(output)
        .finalize_as_transaction();

    let input = unset_input.into_input(UtxoId::new(
        tx1.id(&fuel_tx::ConsensusParameters::DEFAULT.chain_id),
        0,
    ));

    let tx2 = TransactionBuilder::script(vec![], vec![])
        .gas_price(9)
        .gas_limit(GAS_LIMIT)
        .add_input(input)
        .finalize_as_transaction();

    let tx3 = TransactionBuilder::script(vec![], vec![])
        .gas_price(20)
        .gas_limit(GAS_LIMIT)
        .add_input(gas_coin)
        .finalize_as_transaction();

    let tx1_id = tx1.id(&fuel_tx::ConsensusParameters::DEFAULT.chain_id);
    let tx2_id = tx2.id(&fuel_tx::ConsensusParameters::DEFAULT.chain_id);
    let tx1 = check_unwrap_tx(tx1, db.clone(), &txpool.config).await;
    let tx2 = check_unwrap_tx(tx2, db.clone(), &txpool.config).await;
    let tx3 = check_unwrap_tx(tx3, db.clone(), &txpool.config).await;

    txpool
        .insert_inner(tx1.clone())
        .expect("Tx1 should be OK, got Err");
    txpool
        .insert_inner(tx2.clone())
        .expect("Tx2 should be OK, got Err");
    let vec = txpool.insert_inner(tx3).expect("Tx3 should be OK, got Err");
    assert_eq!(
        vec.removed.len(),
        2,
        "Tx1 and Tx2 should be removed:{vec:?}",
    );
    assert_eq!(vec.removed[0].id(), tx1_id, "Tx1 id should be removed");
    assert_eq!(vec.removed[1].id(), tx2_id, "Tx2 id should be removed");
}

#[tokio::test]
async fn more_priced_tx2_removes_tx1_and_more_priced_tx3_removes_tx2() {
    let mut rng = StdRng::seed_from_u64(0);
    let db = MockDb::default();
    let mut txpool = TxPool::new(Default::default(), db.clone());

    let (_, gas_coin) = setup_coin(&mut rng, Some(&txpool.database));

    let tx1 = TransactionBuilder::script(vec![], vec![])
        .gas_price(10)
        .gas_limit(GAS_LIMIT)
        .add_input(gas_coin.clone())
        .finalize_as_transaction();

    let tx2 = TransactionBuilder::script(vec![], vec![])
        .gas_price(11)
        .gas_limit(GAS_LIMIT)
        .add_input(gas_coin.clone())
        .finalize_as_transaction();

    let tx3 = TransactionBuilder::script(vec![], vec![])
        .gas_price(12)
        .gas_limit(GAS_LIMIT)
        .add_input(gas_coin)
        .finalize_as_transaction();

    let tx1 = check_unwrap_tx(tx1, db.clone(), &txpool.config).await;
    let tx2 = check_unwrap_tx(tx2, db.clone(), &txpool.config).await;
    let tx3 = check_unwrap_tx(tx3, db.clone(), &txpool.config).await;

    txpool.insert_inner(tx1).expect("Tx1 should be OK, got Err");
    let squeezed = txpool.insert_inner(tx2).expect("Tx2 should be OK, got Err");
    assert_eq!(squeezed.removed.len(), 1);
    let squeezed = txpool.insert_inner(tx3).expect("Tx3 should be OK, got Err");
    assert_eq!(
        squeezed.removed.len(),
        1,
        "Tx2 should be removed:{squeezed:?}"
    );
}

#[tokio::test]
async fn tx_limit_hit() {
    let mut rng = StdRng::seed_from_u64(0);
    let db = MockDb::default();
    let mut txpool = TxPool::new(
        Config {
            max_tx: 1,
            ..Default::default()
        },
        db.clone(),
    );

    let (_, gas_coin) = setup_coin(&mut rng, Some(&txpool.database));
    let tx1 = TransactionBuilder::script(vec![], vec![])
        .gas_limit(GAS_LIMIT)
        .add_input(gas_coin)
        .add_output(create_coin_output())
        .finalize_as_transaction();

    let (_, gas_coin) = setup_coin(&mut rng, Some(&txpool.database));
    let tx2 = TransactionBuilder::script(vec![], vec![])
        .gas_limit(GAS_LIMIT)
        .add_input(gas_coin)
        .finalize_as_transaction();

    let tx1 = check_unwrap_tx(tx1, db.clone(), &txpool.config).await;
    let tx2 = check_unwrap_tx(tx2, db.clone(), &txpool.config).await;
    txpool.insert_inner(tx1).expect("Tx1 should be Ok, got Err");

    let err = txpool
        .insert_inner(tx2)
        .expect_err("Tx2 should be Err, got Ok");
    assert!(matches!(
        err.downcast_ref::<Error>(),
        Some(Error::NotInsertedLimitHit)
    ));
}

#[tokio::test]
async fn tx_depth_hit() {
    let mut rng = StdRng::seed_from_u64(0);
    let db = MockDb::default();
    let mut txpool = TxPool::new(
        Config {
            max_depth: 2,
            ..Default::default()
        },
        db.clone(),
    );

    let (_, gas_coin) = setup_coin(&mut rng, Some(&txpool.database));
    let (output, unset_input) = create_output_and_input(&mut rng, 10_000);
    let tx1 = TransactionBuilder::script(vec![], vec![])
        .gas_limit(GAS_LIMIT)
        .add_input(gas_coin)
        .add_output(output)
        .finalize_as_transaction();

    let input = unset_input.into_input(UtxoId::new(
        tx1.id(&fuel_tx::ConsensusParameters::DEFAULT.chain_id),
        0,
    ));
    let (output, unset_input) = create_output_and_input(&mut rng, 5_000);
    let tx2 = TransactionBuilder::script(vec![], vec![])
        .gas_limit(GAS_LIMIT)
        .add_input(input)
        .add_output(output)
        .finalize_as_transaction();

    let input = unset_input.into_input(UtxoId::new(
        tx2.id(&fuel_tx::ConsensusParameters::DEFAULT.chain_id),
        0,
    ));
    let tx3 = TransactionBuilder::script(vec![], vec![])
        .gas_limit(GAS_LIMIT)
        .add_input(input)
        .finalize_as_transaction();

    let tx1 = check_unwrap_tx(tx1, db.clone(), &txpool.config).await;
    let tx2 = check_unwrap_tx(tx2, db.clone(), &txpool.config).await;
    let tx3 = check_unwrap_tx(tx3, db.clone(), &txpool.config).await;

    txpool.insert_inner(tx1).expect("Tx1 should be OK, got Err");
    txpool.insert_inner(tx2).expect("Tx2 should be OK, got Err");

    let err = txpool
        .insert_inner(tx3)
        .expect_err("Tx3 should be Err, got Ok");
    assert!(matches!(
        err.downcast_ref::<Error>(),
        Some(Error::NotInsertedMaxDepth)
    ));
}

#[tokio::test]
async fn sorted_out_tx1_2_4() {
    let mut rng = StdRng::seed_from_u64(0);
    let db = MockDb::default();
    let mut txpool = TxPool::new(Default::default(), db.clone());

    let (_, gas_coin) = setup_coin(&mut rng, Some(&txpool.database));
    let tx1 = TransactionBuilder::script(vec![], vec![])
        .gas_price(10)
        .gas_limit(GAS_LIMIT)
        .add_input(gas_coin)
        .finalize_as_transaction();

    let (_, gas_coin) = setup_coin(&mut rng, Some(&txpool.database));
    let tx2 = TransactionBuilder::script(vec![], vec![])
        .gas_price(9)
        .gas_limit(GAS_LIMIT)
        .add_input(gas_coin)
        .finalize_as_transaction();

    let (_, gas_coin) = setup_coin(&mut rng, Some(&txpool.database));
    let tx3 = TransactionBuilder::script(vec![], vec![])
        .gas_price(20)
        .gas_limit(GAS_LIMIT)
        .add_input(gas_coin)
        .finalize_as_transaction();

    let tx1_id = tx1.id(&fuel_tx::ConsensusParameters::DEFAULT.chain_id);
    let tx2_id = tx2.id(&fuel_tx::ConsensusParameters::DEFAULT.chain_id);
    let tx3_id = tx3.id(&fuel_tx::ConsensusParameters::DEFAULT.chain_id);

    let tx1 = check_unwrap_tx(tx1, db.clone(), &txpool.config).await;
    let tx2 = check_unwrap_tx(tx2, db.clone(), &txpool.config).await;
    let tx3 = check_unwrap_tx(tx3, db.clone(), &txpool.config).await;

    txpool.insert_inner(tx1).expect("Tx1 should be Ok, got Err");
    txpool.insert_inner(tx2).expect("Tx2 should be Ok, got Err");
    txpool.insert_inner(tx3).expect("Tx4 should be Ok, got Err");

    let txs = txpool.sorted_includable().collect::<Vec<_>>();

    assert_eq!(txs.len(), 3, "Should have 3 txs");
    assert_eq!(txs[0].id(), tx3_id, "First should be tx3");
    assert_eq!(txs[1].id(), tx1_id, "Second should be tx1");
    assert_eq!(txs[2].id(), tx2_id, "Third should be tx2");
}

#[tokio::test]
async fn find_dependent_tx1_tx2() {
    let mut rng = StdRng::seed_from_u64(0);
    let db = MockDb::default();
    let mut txpool = TxPool::new(Default::default(), db.clone());

    let (_, gas_coin) = setup_coin(&mut rng, Some(&txpool.database));
    let (output, unset_input) = create_output_and_input(&mut rng, 10_000);
    let tx1 = TransactionBuilder::script(vec![], vec![])
        .gas_price(11)
        .gas_limit(GAS_LIMIT)
        .add_input(gas_coin)
        .add_output(output)
        .finalize_as_transaction();

    let input = unset_input.into_input(UtxoId::new(
        tx1.id(&fuel_tx::ConsensusParameters::DEFAULT.chain_id),
        0,
    ));
    let (output, unset_input) = create_output_and_input(&mut rng, 7_500);
    let tx2 = TransactionBuilder::script(vec![], vec![])
        .gas_price(10)
        .gas_limit(GAS_LIMIT)
        .add_input(input)
        .add_output(output)
        .finalize_as_transaction();

    let input = unset_input.into_input(UtxoId::new(
        tx2.id(&fuel_tx::ConsensusParameters::DEFAULT.chain_id),
        0,
    ));
    let tx3 = TransactionBuilder::script(vec![], vec![])
        .gas_price(9)
        .gas_limit(GAS_LIMIT)
        .add_input(input)
        .finalize_as_transaction();

    let tx1_id = tx1.id(&fuel_tx::ConsensusParameters::DEFAULT.chain_id);
    let tx2_id = tx2.id(&fuel_tx::ConsensusParameters::DEFAULT.chain_id);
    let tx3_id = tx3.id(&fuel_tx::ConsensusParameters::DEFAULT.chain_id);

    let tx1 = check_unwrap_tx(tx1, db.clone(), &txpool.config).await;
    let tx2 = check_unwrap_tx(tx2, db.clone(), &txpool.config).await;
    let tx3 = check_unwrap_tx(tx3, db.clone(), &txpool.config).await;

    txpool.insert_inner(tx1).expect("Tx0 should be Ok, got Err");
    txpool.insert_inner(tx2).expect("Tx1 should be Ok, got Err");
    let tx3_result = txpool.insert_inner(tx3).expect("Tx2 should be Ok, got Err");

    let mut seen = HashMap::new();
    txpool
        .dependency()
        .find_dependent(tx3_result.inserted, &mut seen, txpool.txs());

    let mut list: Vec<_> = seen.into_values().collect();
    // sort from high to low price
    list.sort_by_key(|tx| Reverse(tx.price()));
    assert_eq!(list.len(), 3, "We should have three items");
    assert_eq!(list[0].id(), tx1_id, "Tx1 should be first.");
    assert_eq!(list[1].id(), tx2_id, "Tx2 should be second.");
    assert_eq!(list[2].id(), tx3_id, "Tx3 should be third.");
}

#[tokio::test]
async fn tx_at_least_min_gas_price_is_insertable() {
    let mut rng = StdRng::seed_from_u64(0);
    let db = MockDb::default();
    let mut txpool = TxPool::new(
        Config {
            min_gas_price: 10,
            ..Default::default()
        },
        db.clone(),
    );

    let (_, gas_coin) = setup_coin(&mut rng, Some(&txpool.database));
    let tx = TransactionBuilder::script(vec![], vec![])
        .gas_price(10)
        .gas_limit(GAS_LIMIT)
        .add_input(gas_coin)
        .finalize_as_transaction();

    let tx = check_unwrap_tx(tx, txpool.database.clone(), &txpool.config).await;
    txpool.insert_inner(tx).expect("Tx should be Ok, got Err");
}

#[tokio::test]
async fn tx_below_min_gas_price_is_not_insertable() {
    let mut rng = StdRng::seed_from_u64(0);
    let db = MockDb::default();

    let (_, gas_coin) = setup_coin(&mut rng, Some(&db));
    let tx = TransactionBuilder::script(vec![], vec![])
        .gas_price(10)
        .gas_limit(GAS_LIMIT)
        .add_input(gas_coin)
        .finalize_as_transaction();

    let err = check_tx(
        tx,
        db,
        &Config {
            min_gas_price: 11,
            ..Default::default()
        },
    )
    .await
    .expect_err("expected insertion failure");

    assert!(matches!(
        err.root_cause().downcast_ref::<Error>().unwrap(),
        Error::NotInsertedGasPriceTooLow
    ));
}

#[tokio::test]
async fn tx_inserted_into_pool_when_input_message_id_exists_in_db() {
    let (message, input) = create_message_predicate_from_message(5000, 0);

    let tx = TransactionBuilder::script(vec![], vec![])
        .gas_limit(GAS_LIMIT)
        .add_input(input)
        .finalize_as_transaction();

    let db = MockDb::default();
    db.insert_message(message);

    let tx1_id = tx.id(&fuel_tx::ConsensusParameters::DEFAULT.chain_id);
    let mut txpool = TxPool::new(Default::default(), db.clone());

    let tx = check_unwrap_tx(tx, db.clone(), &txpool.config).await;
    txpool.insert_inner(tx).expect("should succeed");

    let tx_info = txpool.find_one(&tx1_id).unwrap();
    assert_eq!(tx_info.tx().id(), tx1_id);
}

#[tokio::test]
async fn tx_rejected_when_input_message_id_is_spent() {
    let (message, input) = create_message_predicate_from_message(5_000, 0);

    let tx = TransactionBuilder::script(vec![], vec![])
        .gas_limit(GAS_LIMIT)
        .add_input(input)
        .finalize_as_transaction();

    let db = MockDb::default();
    db.insert_message(message.clone());
    db.spend_message(*message.id());
    let mut txpool = TxPool::new(Default::default(), db.clone());

    let tx = check_unwrap_tx(tx, db.clone(), &txpool.config).await;
    let err = txpool.insert_inner(tx).expect_err("should fail");

    // check error
    assert!(matches!(
        err.downcast_ref::<Error>(),
        Some(Error::NotInsertedInputMessageSpent(msg_id)) if msg_id == message.id()
    ));
}

#[tokio::test]
async fn tx_rejected_from_pool_when_input_message_id_does_not_exist_in_db() {
    let (message, input) = create_message_predicate_from_message(5000, 0);
    let tx = TransactionBuilder::script(vec![], vec![])
        .gas_limit(GAS_LIMIT)
        .add_input(input)
        .finalize_as_transaction();

    let db = MockDb::default();
    // Do not insert any messages into the DB to ensure there is no matching message for the
    // tx.
    let mut txpool = TxPool::new(Default::default(), db.clone());
    let tx = check_unwrap_tx(tx, db.clone(), &txpool.config).await;
    let err = txpool.insert_inner(tx).expect_err("should fail");

    // check error
    assert!(matches!(
        err.downcast_ref::<Error>(),
        Some(Error::NotInsertedInputMessageUnknown(msg_id)) if msg_id == message.id()
    ));
}

#[tokio::test]
async fn tx_rejected_from_pool_when_gas_price_is_lower_than_another_tx_with_same_message_id(
) {
    let message_amount = 10_000;
    let gas_price_high = 2u64;
    let gas_price_low = 1u64;
    let (message, conflicting_message_input) =
        create_message_predicate_from_message(message_amount, 0);

    let tx_high = TransactionBuilder::script(vec![], vec![])
        .gas_price(gas_price_high)
        .gas_limit(GAS_LIMIT)
        .add_input(conflicting_message_input.clone())
        .finalize_as_transaction();

    let tx_low = TransactionBuilder::script(vec![], vec![])
        .gas_price(gas_price_low)
        .gas_limit(GAS_LIMIT)
        .add_input(conflicting_message_input)
        .finalize_as_transaction();

    let db = MockDb::default();
    db.insert_message(message.clone());

    let mut txpool = TxPool::new(Default::default(), db.clone());

    let tx_high_id = tx_high.id(&fuel_tx::ConsensusParameters::DEFAULT.chain_id);
    let tx_high = check_unwrap_tx(tx_high, db.clone(), &txpool.config).await;

    // Insert a tx for the message id with a high gas amount
    txpool
        .insert_inner(tx_high)
        .expect("expected successful insertion");

    let tx_low = check_unwrap_tx(tx_low, db.clone(), &txpool.config).await;
    // Insert a tx for the message id with a low gas amount
    // Because the new transaction's id matches an existing transaction, we compare the gas
    // prices of both the new and existing transactions. Since the existing transaction's gas
    // price is higher, we must now reject the new transaction.
    let err = txpool.insert_inner(tx_low).expect_err("expected failure");

    // check error
    assert!(matches!(
        err.downcast_ref::<Error>(),
        Some(Error::NotInsertedCollisionMessageId(tx_id, msg_id)) if tx_id == &tx_high_id && msg_id == message.id()
    ));
}

#[tokio::test]
async fn higher_priced_tx_squeezes_out_lower_priced_tx_with_same_message_id() {
    let message_amount = 10_000;
    let gas_price_high = 2u64;
    let gas_price_low = 1u64;
    let (message, conflicting_message_input) =
        create_message_predicate_from_message(message_amount, 0);

    // Insert a tx for the message id with a low gas amount
    let tx_low = TransactionBuilder::script(vec![], vec![])
        .gas_price(gas_price_low)
        .gas_limit(GAS_LIMIT)
        .add_input(conflicting_message_input.clone())
        .finalize_as_transaction();

    let db = MockDb::default();
    db.insert_message(message);

    let mut txpool = TxPool::new(Default::default(), db.clone());
    let tx_low_id = tx_low.id(&fuel_tx::ConsensusParameters::DEFAULT.chain_id);
    let tx_low = check_unwrap_tx(tx_low, db.clone(), &txpool.config).await;
    txpool.insert_inner(tx_low).expect("should succeed");

    // Insert a tx for the message id with a high gas amount
    // Because the new transaction's id matches an existing transaction, we compare the gas
    // prices of both the new and existing transactions. Since the existing transaction's gas
    // price is lower, we accept the new transaction and squeeze out the old transaction.
    let tx_high = TransactionBuilder::script(vec![], vec![])
        .gas_price(gas_price_high)
        .gas_limit(GAS_LIMIT)
        .add_input(conflicting_message_input)
        .finalize_as_transaction();
    let tx_high = check_unwrap_tx(tx_high, db.clone(), &txpool.config).await;
    let squeezed_out_txs = txpool.insert_inner(tx_high).expect("should succeed");

    assert_eq!(squeezed_out_txs.removed.len(), 1);
    assert_eq!(squeezed_out_txs.removed[0].id(), tx_low_id,);
}

#[tokio::test]
async fn message_of_squeezed_out_tx_can_be_resubmitted_at_lower_gas_price() {
    // tx1 (message 1, message 2) gas_price 2
    // tx2 (message 1) gas_price 3
    //   squeezes tx1 with higher gas price
    // tx3 (message 2) gas_price 1
    //   works since tx1 is no longer part of txpool state even though gas price is less

    let (message_1, message_input_1) = create_message_predicate_from_message(10_000, 0);
    let (message_2, message_input_2) = create_message_predicate_from_message(20_000, 1);

    // Insert a tx for the message id with a low gas amount
    let tx1 = TransactionBuilder::script(vec![], vec![])
        .gas_price(2)
        .gas_limit(GAS_LIMIT)
        .add_input(message_input_1.clone())
        .add_input(message_input_2.clone())
        .finalize_as_transaction();

    let tx2 = TransactionBuilder::script(vec![], vec![])
        .gas_price(3)
        .gas_limit(GAS_LIMIT)
        .add_input(message_input_1)
        .finalize_as_transaction();

    let tx3 = TransactionBuilder::script(vec![], vec![])
        .gas_price(1)
        .gas_limit(GAS_LIMIT)
        .add_input(message_input_2)
        .finalize_as_transaction();

    let db = MockDb::default();
    db.insert_message(message_1);
    db.insert_message(message_2);
    let mut txpool = TxPool::new(Default::default(), db.clone());

    let tx1 = check_unwrap_tx(tx1, db.clone(), &txpool.config).await;
    let tx2 = check_unwrap_tx(tx2, db.clone(), &txpool.config).await;
    let tx3 = check_unwrap_tx(tx3, db.clone(), &txpool.config).await;

    txpool.insert_inner(tx1).expect("should succeed");

    txpool.insert_inner(tx2).expect("should succeed");

    txpool.insert_inner(tx3).expect("should succeed");
}

#[tokio::test]
async fn predicates_with_incorrect_owner_fails() {
    let mut rng = StdRng::seed_from_u64(0);
    let db = MockDb::default();
    let mut coin = random_predicate(&mut rng, AssetId::BASE, TEST_COIN_AMOUNT, None);
    if let Input::CoinPredicate(CoinPredicate { owner, .. }) = &mut coin {
        *owner = Address::zeroed();
    }

    let (_, gas_coin) = add_coin_to_state(coin, Some(&db.clone()));
    let tx = TransactionBuilder::script(vec![], vec![])
        .gas_limit(GAS_LIMIT)
        .add_input(gas_coin)
        .finalize_as_transaction();

    let err = check_tx(tx, db.clone(), &Default::default())
        .await
        .expect_err("Transaction should be err, got ok");

    assert!(
        err.to_string().contains("InputPredicateOwner"),
        "unexpected error: {err}",
    )
}

#[tokio::test]
async fn predicate_without_enough_gas_returns_out_of_gas() {
    let mut rng = StdRng::seed_from_u64(0);
    let db = MockDb::default();
    let mut config = Config::default();
    config
        .chain_config
        .transaction_parameters
        .max_gas_per_predicate = 10000;
    config.chain_config.transaction_parameters.max_gas_per_tx = 10000;
    let coin = custom_predicate(
        &mut rng,
        AssetId::BASE,
        TEST_COIN_AMOUNT,
        // forever loop
        vec![op::jmp(RegId::ZERO)].into_iter().collect(),
        None,
    )
    .into_estimated(
        &config.chain_config.transaction_parameters,
        &config.chain_config.gas_costs,
    );

    let (_, gas_coin) = add_coin_to_state(coin, Some(&db.clone()));
    let tx = TransactionBuilder::script(vec![], vec![])
        .gas_limit(GAS_LIMIT)
        .add_input(gas_coin)
        .finalize_as_transaction();

    let err = check_tx(tx, db.clone(), &Default::default())
        .await
        .expect_err("Transaction should be err, got ok");

    assert!(
        err.to_string().contains("PredicateExhaustedGas"),
        "unexpected error: {err}",
    )
}

#[tokio::test]
async fn predicate_that_returns_false_is_invalid() {
    let mut rng = StdRng::seed_from_u64(0);
    let db = MockDb::default();
    let coin = custom_predicate(
        &mut rng,
        AssetId::BASE,
        TEST_COIN_AMOUNT,
        // forever loop
        vec![op::ret(RegId::ZERO)].into_iter().collect(),
        None,
    )
    .into_default_estimated();

    let (_, gas_coin) = add_coin_to_state(coin, Some(&db.clone()));
    let tx = TransactionBuilder::script(vec![], vec![])
        .gas_limit(GAS_LIMIT)
        .add_input(gas_coin)
        .finalize_as_transaction();

    let err = check_tx(tx, db.clone(), &Default::default())
        .await
        .expect_err("Transaction should be err, got ok");

    assert!(
        err.to_string().contains("PredicateVerificationFailed"),
        "unexpected error: {err}",
    )
}
