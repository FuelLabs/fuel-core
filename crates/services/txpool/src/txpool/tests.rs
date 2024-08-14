#![allow(non_snake_case)]

use crate::{
    ports::WasmValidityError,
    service::test_helpers::MockTxPoolGasPrice,
    test_helpers::{
        IntoEstimated,
        MockWasmChecker,
        TextContext,
        TEST_COIN_AMOUNT,
    },
    txpool::test_helpers::{
        create_coin_output,
        create_contract_input,
        create_contract_output,
        create_message_predicate_from_message,
    },
    types::GasPrice,
    Config,
    Error,
};
use fuel_core_types::{
    fuel_asm::{
        op,
        RegId,
        Word,
    },
    fuel_tx::{
        input::coin::CoinPredicate,
        Address,
        AssetId,
        BlobBody,
        BlobId,
        BlobIdExt,
        Bytes32,
        ConsensusParameters,
        Contract,
        Finalizable,
        Input,
        Output,
        PredicateParameters,
        Transaction,
        TransactionBuilder,
        TxParameters,
        UniqueIdentifier,
        UpgradePurpose,
        UtxoId,
    },
    fuel_types::ChainId,
    fuel_vm::{
        checked_transaction::{
            CheckError,
            Checked,
        },
        interpreter::MemoryInstance,
    },
};
use std::vec;

use super::check_single_tx;

const GAS_LIMIT: Word = 100000;

async fn check_unwrap_tx(tx: Transaction, config: &Config) -> Checked<Transaction> {
    let gas_price = 0;
    check_unwrap_tx_with_gas_price(tx, config, gas_price).await
}

async fn check_unwrap_tx_with_gas_price(
    tx: Transaction,
    config: &Config,
    gas_price: GasPrice,
) -> Checked<Transaction> {
    let gas_price_provider = MockTxPoolGasPrice::new(gas_price);
    check_single_tx(
        tx,
        Default::default(),
        config.utxo_validation,
        &ConsensusParameters::default(),
        &gas_price_provider,
        MemoryInstance::new(),
    )
    .await
    .expect("Transaction should be checked")
}

async fn check_tx(
    tx: Transaction,
    config: &Config,
) -> Result<Checked<Transaction>, Error> {
    let gas_price = 0;
    check_tx_with_gas_price(tx, config, gas_price).await
}

async fn check_tx_with_gas_price(
    tx: Transaction,
    config: &Config,
    gas_price: GasPrice,
) -> Result<Checked<Transaction>, Error> {
    let gas_price_provider = MockTxPoolGasPrice::new(gas_price);
    check_single_tx(
        tx,
        Default::default(),
        config.utxo_validation,
        &ConsensusParameters::default(),
        &gas_price_provider,
        MemoryInstance::new(),
    )
    .await
}

#[tokio::test]
async fn insert_simple_tx_succeeds() {
    let mut context = TextContext::default();

    let (_, gas_coin) = context.setup_coin();
    let tx = TransactionBuilder::script(vec![], vec![])
        .script_gas_limit(GAS_LIMIT)
        .add_input(gas_coin)
        .finalize_as_transaction();

    let mut txpool = context.build();
    let tx = check_unwrap_tx(tx, &txpool.config).await;

    txpool
        .insert_single(tx)
        .expect("Transaction should be OK, got Err");
}

#[tokio::test]
async fn insert_simple_tx_with_blacklisted_utxo_id_fails() {
    let mut context = TextContext::default();

    let (_, gas_coin) = context.setup_coin();
    let tx = TransactionBuilder::script(vec![], vec![])
        .script_gas_limit(GAS_LIMIT)
        .add_input(gas_coin.clone())
        .finalize_as_transaction();
    let mut txpool = context.build();
    let tx = check_unwrap_tx(tx, &txpool.config).await;
    let utxo_id = *gas_coin.utxo_id().unwrap();

    // Given
    txpool.config_mut().blacklist.coins.insert(utxo_id);

    // When
    let result = txpool.insert_single(tx);

    // Then
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains(format!("The UTXO `{}` is blacklisted", utxo_id).as_str()));
}

#[tokio::test]
async fn insert_simple_tx_with_blacklisted_owner_fails() {
    let mut context = TextContext::default();

    let (_, gas_coin) = context.setup_coin();
    let tx = TransactionBuilder::script(vec![], vec![])
        .script_gas_limit(GAS_LIMIT)
        .add_input(gas_coin.clone())
        .finalize_as_transaction();
    let mut txpool = context.build();
    let tx = check_unwrap_tx(tx, &txpool.config).await;
    let owner = *gas_coin.input_owner().unwrap();

    // Given
    txpool.config_mut().blacklist.owners.insert(owner);

    // When
    let result = txpool.insert_single(tx);

    // Then
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains(format!("The owner `{}` is blacklisted", owner).as_str()));
}

#[tokio::test]
async fn insert_simple_tx_with_blacklisted_contract_fails() {
    let mut context = TextContext::default();
    let contract_id = Contract::EMPTY_CONTRACT_ID;

    let (_, gas_coin) = context.setup_coin();
    let tx = TransactionBuilder::script(vec![], vec![])
        .script_gas_limit(GAS_LIMIT)
        .add_input(gas_coin.clone())
        .add_input(create_contract_input(
            Default::default(),
            Default::default(),
            contract_id,
        ))
        .add_output(Output::contract(1, Default::default(), Default::default()))
        .finalize_as_transaction();
    let mut txpool = context.build();
    let tx = check_unwrap_tx(tx, &txpool.config).await;

    // Given
    txpool.config_mut().blacklist.contracts.insert(contract_id);

    // When
    let result = txpool.insert_single(tx);

    // Then
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains(format!("The contract `{}` is blacklisted", contract_id).as_str()));
}

#[tokio::test]
async fn insert_simple_tx_with_blacklisted_message_fails() {
    let (message, input) = create_message_predicate_from_message(5000, 0);

    let tx = TransactionBuilder::script(vec![], vec![])
        .script_gas_limit(GAS_LIMIT)
        .add_input(input)
        .finalize_as_transaction();

    let nonce = *message.nonce();
    let mut context = TextContext::default();
    context.database_mut().insert_message(message);
    let mut txpool = context.build();

    let tx = check_unwrap_tx(tx, &txpool.config).await;

    // Given
    txpool.config_mut().blacklist.messages.insert(nonce);

    // When
    let result = txpool.insert_single(tx);

    // Then
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains(format!("The message `{}` is blacklisted", nonce).as_str()));
}

#[tokio::test]
async fn insert_simple_tx_dependency_chain_succeeds() {
    let mut context = TextContext::default();

    let (_, gas_coin) = context.setup_coin();
    let (output, unset_input) = context.create_output_and_input(1);
    let tx1 = TransactionBuilder::script(vec![], vec![])
        .tip(1)
        .max_fee_limit(1)
        .script_gas_limit(GAS_LIMIT)
        .add_input(gas_coin)
        .add_output(output)
        .finalize_as_transaction();

    let (_, gas_coin) = context.setup_coin();
    let input = unset_input.into_input(UtxoId::new(tx1.id(&Default::default()), 0));
    let tx2 = TransactionBuilder::script(vec![], vec![])
        .tip(1)
        .max_fee_limit(1)
        .script_gas_limit(GAS_LIMIT)
        .add_input(input)
        .add_input(gas_coin)
        .finalize_as_transaction();

    let mut txpool = context.build();
    let tx1 = check_unwrap_tx(tx1, &txpool.config).await;
    let tx2 = check_unwrap_tx(tx2, &txpool.config).await;

    txpool
        .insert_single(tx1)
        .expect("Tx1 should be OK, got Err");
    txpool
        .insert_single(tx2)
        .expect("Tx2 dependent should be OK, got Err");
}

#[tokio::test]
async fn faulty_t2_collided_on_contract_id_from_tx1() {
    let mut context = TextContext::default();

    let contract_id = Contract::EMPTY_CONTRACT_ID;

    // contract creation tx
    let (_, gas_coin) = context.setup_coin();
    let (output, unset_input) = context.create_output_and_input(10);
    let tx = TransactionBuilder::create(
        Default::default(),
        Default::default(),
        Default::default(),
    )
    .tip(10)
    .max_fee_limit(10)
    .add_input(gas_coin)
    .add_output(create_contract_output(contract_id))
    .add_output(output)
    .finalize_as_transaction();

    let (_, gas_coin) = context.setup_coin();
    let input = unset_input.into_input(UtxoId::new(tx.id(&Default::default()), 1));

    // attempt to insert a different creation tx with a valid dependency on the first tx,
    // but with a conflicting output contract id
    let tx_faulty = TransactionBuilder::create(
        Default::default(),
        Default::default(),
        Default::default(),
    )
    .tip(9)
    .max_fee_limit(9)
    .add_input(gas_coin)
    .add_input(input)
    .add_output(create_contract_output(contract_id))
    .add_output(output)
    .finalize_as_transaction();

    let mut txpool = context.build();
    let tx = check_unwrap_tx(tx, &txpool.config).await;
    txpool.insert_single(tx).expect("Tx1 should be Ok, got Err");

    let tx_faulty = check_unwrap_tx(tx_faulty, &txpool.config).await;

    let err = txpool
        .insert_single(tx_faulty)
        .expect_err("Tx2 should be Err, got Ok");
    assert!(matches!(
        err,
        Error::NotInsertedCollisionContractId(id) if id == contract_id
    ));
}

#[tokio::test]
async fn fail_to_insert_tx_with_dependency_on_invalid_utxo_type() {
    let mut context = TextContext::default();

    let contract_id = Contract::EMPTY_CONTRACT_ID;
    let (_, gas_coin) = context.setup_coin();
    let tx_faulty = TransactionBuilder::create(
        Default::default(),
        Default::default(),
        Default::default(),
    )
    .add_input(gas_coin)
    .add_output(create_contract_output(contract_id))
    .finalize_as_transaction();

    // create a second transaction with utxo id referring to
    // the wrong type of utxo (contract instead of coin)
    let tx = TransactionBuilder::script(vec![], vec![])
        .tip(1)
        .max_fee_limit(1)
        .max_fee_limit(1)
        .script_gas_limit(GAS_LIMIT)
        .add_input(context.random_predicate(
            AssetId::BASE,
            TEST_COIN_AMOUNT,
            Some(UtxoId::new(tx_faulty.id(&Default::default()), 0)),
        ))
        .finalize_as_transaction();

    let mut txpool = context.build();
    let tx_faulty_id = tx_faulty.id(&ChainId::default());
    let tx_faulty = check_unwrap_tx(tx_faulty, &txpool.config).await;

    txpool
        .insert_single(tx_faulty.clone())
        .expect("Tx1 should be Ok, got Err");

    let tx = check_unwrap_tx(tx, &txpool.config).await;

    let err = txpool
        .insert_single(tx)
        .expect_err("Tx2 should be Err, got Ok");
    assert!(matches!(
        err,
        Error::NotInsertedInputUtxoIdNotDoesNotExist(id) if id == UtxoId::new(tx_faulty_id, 0)
    ));
}

#[tokio::test]
async fn not_inserted_known_tx() {
    let config = Config {
        utxo_validation: false,
        ..Default::default()
    };
    let context = TextContext::default().config(config);
    let mut txpool = context.build();

    let tx = TransactionBuilder::script(vec![], vec![])
        .add_random_fee_input()
        .finalize()
        .into();
    let tx = check_unwrap_tx(tx, &txpool.config).await;

    txpool
        .insert_single(tx.clone())
        .expect("Tx1 should be Ok, got Err");

    let err = txpool
        .insert_single(tx)
        .expect_err("Second insertion of Tx1 should be Err, got Ok");
    assert!(matches!(err, Error::NotInsertedTxKnown));
}

#[tokio::test]
async fn try_to_insert_tx2_missing_utxo() {
    let mut context = TextContext::default();

    let input = context.random_predicate(AssetId::BASE, TEST_COIN_AMOUNT, None);
    let tx = TransactionBuilder::script(vec![], vec![])
        .tip(10)
        .max_fee_limit(10)
        .script_gas_limit(GAS_LIMIT)
        .add_input(input)
        .finalize_as_transaction();

    let mut txpool = context.build();
    let tx = check_unwrap_tx(tx, &txpool.config).await;

    let err = txpool
        .insert_single(tx)
        .expect_err("Tx should be Err, got Ok");
    assert!(matches!(
        err,
        Error::NotInsertedInputUtxoIdNotDoesNotExist(_)
    ));
}

#[tokio::test]
async fn higher_priced_tx_removes_lower_priced_tx() {
    let mut context = TextContext::default();

    let (_, coin_input) = context.setup_coin();

    let tx1 = TransactionBuilder::script(vec![], vec![])
        .tip(10)
        .max_fee_limit(10)
        .script_gas_limit(GAS_LIMIT)
        .add_input(coin_input.clone())
        .finalize_as_transaction();

    let tx2 = TransactionBuilder::script(vec![], vec![])
        .tip(20)
        .max_fee_limit(20)
        .script_gas_limit(GAS_LIMIT)
        .add_input(coin_input)
        .finalize_as_transaction();

    let tx1_id = tx1.id(&ChainId::default());
    let mut txpool = context.build();
    let tx1 = check_unwrap_tx(tx1, &txpool.config).await;

    txpool
        .insert_single(tx1.clone())
        .expect("Tx1 should be Ok, got Err");

    let tx2 = check_unwrap_tx(tx2, &txpool.config).await;

    let vec = txpool
        .insert_single(tx2)
        .expect("Tx2 should be Ok, got Err");
    assert_eq!(vec.removed[0].id(), tx1_id, "Tx1 id should be removed");
}

#[tokio::test]
async fn underpriced_tx1_not_included_coin_collision() {
    let mut context = TextContext::default();

    let (_, gas_coin) = context.setup_coin();
    let (output, unset_input) = context.create_output_and_input(20);
    let tx1 = TransactionBuilder::script(vec![], vec![])
        .tip(20)
        .max_fee_limit(20)
        .script_gas_limit(GAS_LIMIT)
        .add_input(gas_coin)
        .add_output(output)
        .finalize_as_transaction();

    let input = unset_input.into_input(UtxoId::new(tx1.id(&Default::default()), 0));

    let tx2 = TransactionBuilder::script(vec![], vec![])
        .tip(20)
        .max_fee_limit(20)
        .script_gas_limit(GAS_LIMIT)
        .add_input(input.clone())
        .finalize_as_transaction();

    let tx3 = TransactionBuilder::script(vec![], vec![])
        .tip(10)
        .max_fee_limit(10)
        .script_gas_limit(GAS_LIMIT)
        .add_input(input)
        .finalize_as_transaction();

    let mut txpool = context.build();
    let tx1_checked = check_unwrap_tx(tx1.clone(), txpool.config()).await;
    txpool
        .insert_single(tx1_checked)
        .expect("Tx1 should be Ok, got Err");

    let tx2_checked = check_unwrap_tx(tx2.clone(), txpool.config()).await;
    txpool
        .insert_single(tx2_checked)
        .expect("Tx2 should be Ok, got Err");

    let tx3_checked = check_unwrap_tx(tx3, txpool.config()).await;
    let err = txpool
        .insert_single(tx3_checked)
        .expect_err("Tx3 should be Err, got Ok");
    assert!(matches!(
        err,
        Error::NotInsertedCollision(id, utxo_id) if id == tx2.id(&Default::default()) && utxo_id == UtxoId::new(tx1.id(&Default::default()), 0)
    ));
}

#[tokio::test]
async fn overpriced_tx_contract_input_not_inserted() {
    let mut context = TextContext::default();

    let contract_id = Contract::EMPTY_CONTRACT_ID;
    let (_, gas_funds) = context.setup_coin();
    let tx1 = TransactionBuilder::create(
        Default::default(),
        Default::default(),
        Default::default(),
    )
    .tip(10)
    .max_fee_limit(10)
    .add_input(gas_funds)
    .add_output(create_contract_output(contract_id))
    .finalize_as_transaction();

    let (_, gas_funds) = context.setup_coin();
    let tx2 = TransactionBuilder::script(vec![], vec![])
        .tip(11)
        .max_fee_limit(11)
        .script_gas_limit(GAS_LIMIT)
        .add_input(gas_funds)
        .add_input(create_contract_input(
            Default::default(),
            Default::default(),
            contract_id,
        ))
        .add_output(Output::contract(1, Default::default(), Default::default()))
        .finalize_as_transaction();

    let mut txpool = context.build();
    let tx1 = check_unwrap_tx(tx1, &txpool.config).await;
    txpool
        .insert_single(tx1)
        .expect("Tx1 should be Ok, got err");

    let tx2 = check_unwrap_tx(tx2, &txpool.config).await;
    let err = txpool
        .insert_single(tx2)
        .expect_err("Tx2 should be Err, got Ok");
    assert!(
        matches!(
            err,
            Error::NotInsertedContractPricedLower(id) if id == contract_id
        ),
        "wrong err {err:?}"
    );
}

#[tokio::test]
async fn dependent_contract_input_inserted() {
    let mut context = TextContext::default();

    let contract_id = Contract::EMPTY_CONTRACT_ID;
    let (_, gas_funds) = context.setup_coin();
    let tx1 = TransactionBuilder::create(
        Default::default(),
        Default::default(),
        Default::default(),
    )
    .tip(10)
    .max_fee_limit(10)
    .add_input(gas_funds)
    .add_output(create_contract_output(contract_id))
    .finalize_as_transaction();

    let (_, gas_funds) = context.setup_coin();
    let tx2 = TransactionBuilder::script(vec![], vec![])
        .tip(10)
        .max_fee_limit(10)
        .script_gas_limit(GAS_LIMIT)
        .add_input(gas_funds)
        .add_input(create_contract_input(
            Default::default(),
            Default::default(),
            contract_id,
        ))
        .add_output(Output::contract(1, Default::default(), Default::default()))
        .finalize_as_transaction();

    let mut txpool = context.build();
    let tx1 = check_unwrap_tx(tx1, &txpool.config).await;
    let tx2 = check_unwrap_tx(tx2, &txpool.config).await;
    txpool
        .insert_single(tx1)
        .expect("Tx1 should be Ok, got Err");
    txpool
        .insert_single(tx2)
        .expect("Tx2 should be Ok, got Err");
}

#[tokio::test]
async fn more_priced_tx3_removes_tx1_and_dependent_tx2() {
    let mut context = TextContext::default();

    let (_, gas_coin) = context.setup_coin();

    let (output, unset_input) = context.create_output_and_input(10);
    let tx1 = TransactionBuilder::script(vec![], vec![])
        .tip(10)
        .max_fee_limit(10)
        .script_gas_limit(GAS_LIMIT)
        .add_input(gas_coin.clone())
        .add_output(output)
        .finalize_as_transaction();

    let input = unset_input.into_input(UtxoId::new(tx1.id(&Default::default()), 0));

    let tx2 = TransactionBuilder::script(vec![], vec![])
        .tip(9)
        .max_fee_limit(9)
        .script_gas_limit(GAS_LIMIT)
        .add_input(input)
        .finalize_as_transaction();

    let tx3 = TransactionBuilder::script(vec![], vec![])
        .tip(20)
        .max_fee_limit(20)
        .script_gas_limit(GAS_LIMIT)
        .add_input(gas_coin)
        .finalize_as_transaction();

    let tx1_id = tx1.id(&ChainId::default());
    let tx2_id = tx2.id(&ChainId::default());
    let mut txpool = context.build();
    let tx1 = check_unwrap_tx(tx1, &txpool.config).await;
    let tx2 = check_unwrap_tx(tx2, &txpool.config).await;
    let tx3 = check_unwrap_tx(tx3, &txpool.config).await;

    txpool
        .insert_single(tx1.clone())
        .expect("Tx1 should be OK, got Err");
    txpool
        .insert_single(tx2.clone())
        .expect("Tx2 should be OK, got Err");
    let vec = txpool
        .insert_single(tx3)
        .expect("Tx3 should be OK, got Err");
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
    let mut context = TextContext::default();

    let (_, gas_coin) = context.setup_coin();

    let tx1 = TransactionBuilder::script(vec![], vec![])
        .tip(10)
        .max_fee_limit(10)
        .script_gas_limit(GAS_LIMIT)
        .add_input(gas_coin.clone())
        .finalize_as_transaction();

    let tx2 = TransactionBuilder::script(vec![], vec![])
        .tip(11)
        .max_fee_limit(11)
        .script_gas_limit(GAS_LIMIT)
        .add_input(gas_coin.clone())
        .finalize_as_transaction();

    let tx3 = TransactionBuilder::script(vec![], vec![])
        .tip(12)
        .max_fee_limit(12)
        .script_gas_limit(GAS_LIMIT)
        .add_input(gas_coin)
        .finalize_as_transaction();

    let mut txpool = context.build();
    let tx1 = check_unwrap_tx(tx1, &txpool.config).await;
    let tx2 = check_unwrap_tx(tx2, &txpool.config).await;
    let tx3 = check_unwrap_tx(tx3, &txpool.config).await;

    txpool
        .insert_single(tx1)
        .expect("Tx1 should be OK, got Err");
    let squeezed = txpool
        .insert_single(tx2)
        .expect("Tx2 should be OK, got Err");
    assert_eq!(squeezed.removed.len(), 1);
    let squeezed = txpool
        .insert_single(tx3)
        .expect("Tx3 should be OK, got Err");
    assert_eq!(
        squeezed.removed.len(),
        1,
        "Tx2 should be removed:{squeezed:?}"
    );
}

#[tokio::test]
async fn tx_limit_hit() {
    let mut context = TextContext::default().config(Config {
        max_tx: 1,
        ..Default::default()
    });

    let (_, gas_coin) = context.setup_coin();
    let tx1 = TransactionBuilder::script(vec![], vec![])
        .script_gas_limit(GAS_LIMIT)
        .add_input(gas_coin)
        .add_output(create_coin_output())
        .finalize_as_transaction();

    let (_, gas_coin) = context.setup_coin();
    let tx2 = TransactionBuilder::script(vec![], vec![])
        .script_gas_limit(GAS_LIMIT)
        .add_input(gas_coin)
        .finalize_as_transaction();

    let mut txpool = context.build();
    let tx1 = check_unwrap_tx(tx1, &txpool.config).await;
    let tx2 = check_unwrap_tx(tx2, &txpool.config).await;
    txpool
        .insert_single(tx1)
        .expect("Tx1 should be Ok, got Err");

    let err = txpool
        .insert_single(tx2)
        .expect_err("Tx2 should be Err, got Ok");
    assert!(matches!(err, Error::NotInsertedLimitHit));
}

#[tokio::test]
async fn tx_depth_hit() {
    let mut context = TextContext::default().config(Config {
        max_depth: 2,
        ..Default::default()
    });

    let (_, gas_coin) = context.setup_coin();
    let (output, unset_input) = context.create_output_and_input(10_000);
    let tx1 = TransactionBuilder::script(vec![], vec![])
        .script_gas_limit(GAS_LIMIT)
        .add_input(gas_coin)
        .add_output(output)
        .finalize_as_transaction();

    let input = unset_input.into_input(UtxoId::new(tx1.id(&Default::default()), 0));
    let (output, unset_input) = context.create_output_and_input(5_000);
    let tx2 = TransactionBuilder::script(vec![], vec![])
        .script_gas_limit(GAS_LIMIT)
        .add_input(input)
        .add_output(output)
        .finalize_as_transaction();

    let input = unset_input.into_input(UtxoId::new(tx2.id(&Default::default()), 0));
    let tx3 = TransactionBuilder::script(vec![], vec![])
        .script_gas_limit(GAS_LIMIT)
        .add_input(input)
        .finalize_as_transaction();

    let mut txpool = context.build();
    let tx1 = check_unwrap_tx(tx1, &txpool.config).await;
    let tx2 = check_unwrap_tx(tx2, &txpool.config).await;
    let tx3 = check_unwrap_tx(tx3, &txpool.config).await;

    txpool
        .insert_single(tx1)
        .expect("Tx1 should be OK, got Err");
    txpool
        .insert_single(tx2)
        .expect("Tx2 should be OK, got Err");

    let err = txpool
        .insert_single(tx3)
        .expect_err("Tx3 should be Err, got Ok");
    assert!(matches!(err, Error::NotInsertedMaxDepth));
}

#[tokio::test]
async fn sorted_out_tx1_2_3() {
    let mut context = TextContext::default();

    let (_, gas_coin) = context.setup_coin();
    let tx1 = TransactionBuilder::script(vec![], vec![])
        .tip(10)
        .max_fee_limit(10)
        .script_gas_limit(GAS_LIMIT)
        .add_input(gas_coin)
        .finalize_as_transaction();

    let (_, gas_coin) = context.setup_coin();
    let tx2 = TransactionBuilder::script(vec![], vec![])
        .tip(9)
        .max_fee_limit(9)
        .script_gas_limit(GAS_LIMIT)
        .add_input(gas_coin)
        .finalize_as_transaction();

    let (_, gas_coin) = context.setup_coin();
    let tx3 = TransactionBuilder::script(vec![], vec![])
        .tip(20)
        .max_fee_limit(20)
        .script_gas_limit(GAS_LIMIT)
        .add_input(gas_coin)
        .finalize_as_transaction();

    let tx1_id = tx1.id(&ChainId::default());
    let tx2_id = tx2.id(&ChainId::default());
    let tx3_id = tx3.id(&ChainId::default());

    let mut txpool = context.build();
    let tx1 = check_unwrap_tx(tx1, &txpool.config).await;
    let tx2 = check_unwrap_tx(tx2, &txpool.config).await;
    let tx3 = check_unwrap_tx(tx3, &txpool.config).await;

    txpool
        .insert_single(tx1)
        .expect("Tx1 should be Ok, got Err");
    txpool
        .insert_single(tx2)
        .expect("Tx2 should be Ok, got Err");
    txpool
        .insert_single(tx3)
        .expect("Tx3 should be Ok, got Err");

    let txs = txpool.sorted_includable().collect::<Vec<_>>();

    assert_eq!(txs.len(), 3, "Should have 3 txs");
    assert_eq!(txs[0].id(), tx3_id, "First should be tx3");
    assert_eq!(txs[1].id(), tx1_id, "Second should be tx1");
    assert_eq!(txs[2].id(), tx2_id, "Third should be tx2");
}

#[tokio::test]
async fn sorted_out_tx_same_tips() {
    let mut context = TextContext::default();

    let (_, gas_coin) = context.setup_coin();
    let tx1 = TransactionBuilder::script(vec![], vec![])
        .tip(10)
        .max_fee_limit(10)
        .script_gas_limit(GAS_LIMIT)
        .add_input(gas_coin)
        .finalize_as_transaction();

    let (_, gas_coin) = context.setup_coin();
    let tx2 = TransactionBuilder::script(vec![], vec![])
        .tip(10)
        .max_fee_limit(10)
        .script_gas_limit(GAS_LIMIT / 2)
        .add_input(gas_coin)
        .finalize_as_transaction();

    let (_, gas_coin) = context.setup_coin();
    let tx3 = TransactionBuilder::script(vec![], vec![])
        .tip(10)
        .max_fee_limit(10)
        .script_gas_limit(GAS_LIMIT / 4)
        .add_input(gas_coin)
        .finalize_as_transaction();

    let tx1_id = tx1.id(&ChainId::default());
    let tx2_id = tx2.id(&ChainId::default());
    let tx3_id = tx3.id(&ChainId::default());

    let mut txpool = context.build();
    let tx1 = check_unwrap_tx(tx1, &txpool.config).await;
    let tx2 = check_unwrap_tx(tx2, &txpool.config).await;
    let tx3 = check_unwrap_tx(tx3, &txpool.config).await;

    txpool
        .insert_single(tx1)
        .expect("Tx1 should be Ok, got Err");
    txpool
        .insert_single(tx2)
        .expect("Tx2 should be Ok, got Err");
    txpool
        .insert_single(tx3)
        .expect("Tx4 should be Ok, got Err");

    let txs = txpool.sorted_includable().collect::<Vec<_>>();

    assert_eq!(txs.len(), 3, "Should have 3 txs");
    assert_eq!(txs[0].id(), tx3_id, "First should be tx3");
    assert_eq!(txs[1].id(), tx2_id, "Second should be tx2");
    assert_eq!(txs[2].id(), tx1_id, "Third should be tx1");
}

#[tokio::test]
async fn sorted_out_tx_profitable_ratios() {
    let mut context = TextContext::default();

    let (_, gas_coin) = context.setup_coin();
    let tx1 = TransactionBuilder::script(vec![], vec![])
        .tip(4)
        .max_fee_limit(4)
        .script_gas_limit(GAS_LIMIT)
        .add_input(gas_coin)
        .finalize_as_transaction();

    let (_, gas_coin) = context.setup_coin();
    let tx2 = TransactionBuilder::script(vec![], vec![])
        .tip(2)
        .max_fee_limit(2)
        .script_gas_limit(GAS_LIMIT / 10)
        .add_input(gas_coin)
        .finalize_as_transaction();

    let (_, gas_coin) = context.setup_coin();
    let tx3 = TransactionBuilder::script(vec![], vec![])
        .tip(1)
        .max_fee_limit(1)
        .script_gas_limit(GAS_LIMIT / 100)
        .add_input(gas_coin)
        .finalize_as_transaction();

    let tx1_id = tx1.id(&ChainId::default());
    let tx2_id = tx2.id(&ChainId::default());
    let tx3_id = tx3.id(&ChainId::default());

    let mut txpool = context.build();
    let tx1 = check_unwrap_tx(tx1, &txpool.config).await;
    let tx2 = check_unwrap_tx(tx2, &txpool.config).await;
    let tx3 = check_unwrap_tx(tx3, &txpool.config).await;

    txpool
        .insert_single(tx1)
        .expect("Tx1 should be Ok, got Err");
    txpool
        .insert_single(tx2)
        .expect("Tx2 should be Ok, got Err");
    txpool
        .insert_single(tx3)
        .expect("Tx4 should be Ok, got Err");

    let txs = txpool.sorted_includable().collect::<Vec<_>>();

    assert_eq!(txs.len(), 3, "Should have 3 txs");
    assert_eq!(txs[0].id(), tx3_id, "First should be tx3");
    assert_eq!(txs[1].id(), tx2_id, "Second should be tx2");
    assert_eq!(txs[2].id(), tx1_id, "Third should be tx1");
}

#[tokio::test]
async fn sorted_out_tx_by_creation_instant() {
    let mut context = TextContext::default();

    let (_, gas_coin) = context.setup_coin();
    let tx1 = TransactionBuilder::script(vec![], vec![])
        .tip(4)
        .max_fee_limit(4)
        .script_gas_limit(GAS_LIMIT)
        .add_input(gas_coin)
        .finalize_as_transaction();

    let (_, gas_coin) = context.setup_coin();
    let tx2 = TransactionBuilder::script(vec![], vec![])
        .tip(4)
        .max_fee_limit(4)
        .script_gas_limit(GAS_LIMIT)
        .add_input(gas_coin)
        .finalize_as_transaction();

    let (_, gas_coin) = context.setup_coin();
    let tx3 = TransactionBuilder::script(vec![], vec![])
        .tip(4)
        .max_fee_limit(4)
        .script_gas_limit(GAS_LIMIT)
        .add_input(gas_coin)
        .finalize_as_transaction();

    let (_, gas_coin) = context.setup_coin();
    let tx4 = TransactionBuilder::script(vec![], vec![])
        .tip(4)
        .max_fee_limit(4)
        .script_gas_limit(GAS_LIMIT)
        .add_input(gas_coin)
        .finalize_as_transaction();

    let tx1_id = tx1.id(&ChainId::default());
    let tx2_id = tx2.id(&ChainId::default());
    let tx3_id = tx3.id(&ChainId::default());
    let tx4_id = tx4.id(&ChainId::default());

    let mut txpool = context.build();
    let tx1 = check_unwrap_tx(tx1, &txpool.config).await;
    let tx2 = check_unwrap_tx(tx2, &txpool.config).await;
    let tx3 = check_unwrap_tx(tx3, &txpool.config).await;
    let tx4 = check_unwrap_tx(tx4, &txpool.config).await;

    txpool
        .insert_single(tx1)
        .expect("Tx1 should be Ok, got Err");
    txpool
        .insert_single(tx2)
        .expect("Tx2 should be Ok, got Err");
    txpool
        .insert_single(tx3)
        .expect("Tx4 should be Ok, got Err");
    txpool
        .insert_single(tx4)
        .expect("Tx4 should be Ok, got Err");

    let txs = txpool.sorted_includable().collect::<Vec<_>>();

    // This order doesn't match the lexicographical order of the tx ids
    // and so it verifies that the txs are sorted by creation instant
    // The newest tx should be first
    assert_eq!(txs.len(), 4, "Should have 4 txs");
    assert_eq!(txs[0].id(), tx1_id, "First should be tx1");
    assert_eq!(txs[1].id(), tx2_id, "Second should be tx2");
    assert_eq!(txs[2].id(), tx3_id, "Third should be tx3");
    assert_eq!(txs[3].id(), tx4_id, "Fourth should be tx4");
}

#[tokio::test]
async fn tx_at_least_min_gas_price_is_insertable() {
    let gas_price = 10;
    let mut context = TextContext::default().config(Config {
        ..Default::default()
    });

    let (_, gas_coin) = context.setup_coin();
    let tx = TransactionBuilder::script(vec![], vec![])
        .tip(10)
        .max_fee_limit(1000)
        .script_gas_limit(GAS_LIMIT)
        .add_input(gas_coin)
        .finalize_as_transaction();

    let mut txpool = context.build();
    let tx = check_unwrap_tx_with_gas_price(tx, &txpool.config, gas_price).await;
    txpool.insert_single(tx).expect("Tx should be Ok, got Err");
}

#[tokio::test]
async fn tx_below_min_gas_price_is_not_insertable() {
    let mut context = TextContext::default();

    let gas_coin = context.random_predicate(AssetId::BASE, TEST_COIN_AMOUNT, None);
    let tx = TransactionBuilder::script(vec![], vec![])
        .tip(10)
        .max_fee_limit(10)
        .script_gas_limit(GAS_LIMIT)
        .add_input(gas_coin)
        .finalize_as_transaction();
    let gas_price = 11;

    let err = check_tx_with_gas_price(
        tx,
        &Config {
            ..Default::default()
        },
        gas_price,
    )
    .await
    .expect_err("expected insertion failure");

    assert!(matches!(
        err,
        Error::ConsensusValidity(CheckError::InsufficientMaxFee { .. })
    ));
}

#[tokio::test]
async fn tx_inserted_into_pool_when_input_message_id_exists_in_db() {
    let mut context = TextContext::default();
    let (message, input) = create_message_predicate_from_message(5000, 0);

    let tx = TransactionBuilder::script(vec![], vec![])
        .script_gas_limit(GAS_LIMIT)
        .add_input(input)
        .finalize_as_transaction();

    context.database_mut().insert_message(message);

    let tx1_id = tx.id(&ChainId::default());
    let mut txpool = context.build();

    let tx = check_unwrap_tx(tx, &txpool.config).await;
    txpool.insert_single(tx).expect("should succeed");

    let tx_info = txpool.find_one(&tx1_id).unwrap();
    assert_eq!(tx_info.tx().id(), tx1_id);
}

#[tokio::test]
async fn tx_rejected_from_pool_when_input_message_id_does_not_exist_in_db() {
    let context = TextContext::default();
    let (message, input) = create_message_predicate_from_message(5000, 0);
    let tx = TransactionBuilder::script(vec![], vec![])
        .script_gas_limit(GAS_LIMIT)
        .add_input(input)
        .finalize_as_transaction();

    // Do not insert any messages into the DB to ensure there is no matching message for the
    // tx.
    let mut txpool = context.build();
    let tx = check_unwrap_tx(tx, &txpool.config).await;
    let err = txpool.insert_single(tx).expect_err("should fail");

    // check error
    assert!(matches!(
        err,
        Error::NotInsertedInputMessageUnknown(msg_id) if msg_id == *message.id()
    ));
}

#[tokio::test]
async fn tx_rejected_from_pool_when_gas_price_is_lower_than_another_tx_with_same_message_id(
) {
    let mut context = TextContext::default();
    let message_amount = 10_000;
    let max_fee_limit = 10u64;
    let gas_price_high = 2u64;
    let gas_price_low = 1u64;
    let (message, conflicting_message_input) =
        create_message_predicate_from_message(message_amount, 0);

    let tx_high = TransactionBuilder::script(vec![], vec![])
        .tip(gas_price_high)
        .max_fee_limit(max_fee_limit)
        .script_gas_limit(GAS_LIMIT)
        .add_input(conflicting_message_input.clone())
        .finalize_as_transaction();

    let tx_low = TransactionBuilder::script(vec![], vec![])
        .tip(gas_price_low)
        .max_fee_limit(max_fee_limit)
        .script_gas_limit(GAS_LIMIT)
        .add_input(conflicting_message_input)
        .finalize_as_transaction();

    context.database_mut().insert_message(message.clone());

    let mut txpool = context.build();

    let tx_high_id = tx_high.id(&ChainId::default());
    let tx_high =
        check_unwrap_tx_with_gas_price(tx_high, &txpool.config, gas_price_high).await;

    // Insert a tx for the message id with a high gas amount
    txpool
        .insert_single(tx_high)
        .expect("expected successful insertion");

    let tx_low =
        check_unwrap_tx_with_gas_price(tx_low, &txpool.config, gas_price_low).await;
    // Insert a tx for the message id with a low gas amount
    // Because the new transaction's id matches an existing transaction, we compare the gas
    // prices of both the new and existing transactions. Since the existing transaction's gas
    // price is higher, we must now reject the new transaction.
    let err = txpool.insert_single(tx_low).expect_err("expected failure");

    // check error
    assert!(matches!(
        err,
        Error::NotInsertedCollisionMessageId(tx_id, msg_id) if tx_id == tx_high_id && msg_id == *message.id()
    ));
}

#[tokio::test]
async fn higher_priced_tx_squeezes_out_lower_priced_tx_with_same_message_id() {
    let mut context = TextContext::default();
    let message_amount = 10_000;
    let gas_price_high = 2u64;
    let max_fee_limit = 10u64;
    let gas_price_low = 1u64;
    let (message, conflicting_message_input) =
        create_message_predicate_from_message(message_amount, 0);

    // Insert a tx for the message id with a low gas amount
    let tx_low = TransactionBuilder::script(vec![], vec![])
        .tip(gas_price_low)
        .max_fee_limit(max_fee_limit)
        .script_gas_limit(GAS_LIMIT)
        .add_input(conflicting_message_input.clone())
        .finalize_as_transaction();

    context.database_mut().insert_message(message);

    let mut txpool = context.build();
    let tx_low_id = tx_low.id(&ChainId::default());
    let tx_low =
        check_unwrap_tx_with_gas_price(tx_low, &txpool.config, gas_price_low).await;
    txpool.insert_single(tx_low).expect("should succeed");

    // Insert a tx for the message id with a high gas amount
    // Because the new transaction's id matches an existing transaction, we compare the gas
    // prices of both the new and existing transactions. Since the existing transaction's gas
    // price is lower, we accept the new transaction and squeeze out the old transaction.
    let tx_high = TransactionBuilder::script(vec![], vec![])
        .tip(gas_price_high)
        .max_fee_limit(max_fee_limit)
        .script_gas_limit(GAS_LIMIT)
        .add_input(conflicting_message_input)
        .finalize_as_transaction();
    let tx_high =
        check_unwrap_tx_with_gas_price(tx_high, &txpool.config, gas_price_high).await;
    let squeezed_out_txs = txpool.insert_single(tx_high).expect("should succeed");

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

    let mut context = TextContext::default();
    let (message_1, message_input_1) = create_message_predicate_from_message(10_000, 0);
    let (message_2, message_input_2) = create_message_predicate_from_message(20_000, 1);

    // Insert a tx for the message id with a low gas amount
    let tx1 = TransactionBuilder::script(vec![], vec![])
        .tip(2)
        .max_fee_limit(2)
        .script_gas_limit(GAS_LIMIT)
        .add_input(message_input_1.clone())
        .add_input(message_input_2.clone())
        .finalize_as_transaction();

    let tx2 = TransactionBuilder::script(vec![], vec![])
        .tip(3)
        .max_fee_limit(3)
        .script_gas_limit(GAS_LIMIT)
        .add_input(message_input_1)
        .finalize_as_transaction();

    let tx3 = TransactionBuilder::script(vec![], vec![])
        .tip(1)
        .max_fee_limit(1)
        .script_gas_limit(GAS_LIMIT)
        .add_input(message_input_2)
        .finalize_as_transaction();

    context.database_mut().insert_message(message_1);
    context.database_mut().insert_message(message_2);
    let mut txpool = context.build();

    let tx1 = check_unwrap_tx(tx1, &txpool.config).await;
    let tx2 = check_unwrap_tx(tx2, &txpool.config).await;
    let tx3 = check_unwrap_tx(tx3, &txpool.config).await;

    txpool.insert_single(tx1).expect("should succeed");

    txpool.insert_single(tx2).expect("should succeed");

    txpool.insert_single(tx3).expect("should succeed");
}

#[tokio::test]
async fn predicates_with_incorrect_owner_fails() {
    let mut context = TextContext::default();
    let mut coin = context.random_predicate(AssetId::BASE, TEST_COIN_AMOUNT, None);
    if let Input::CoinPredicate(CoinPredicate { owner, .. }) = &mut coin {
        *owner = Address::zeroed();
    }

    let tx = TransactionBuilder::script(vec![], vec![])
        .script_gas_limit(GAS_LIMIT)
        .add_input(coin)
        .finalize_as_transaction();

    let err = check_tx(tx, &Default::default())
        .await
        .expect_err("Transaction should be err, got ok");

    assert!(
        format!("{err:?}").contains("InputPredicateOwner"),
        "unexpected error: {err:?}",
    )
}

#[tokio::test]
async fn predicate_without_enough_gas_returns_out_of_gas() {
    let mut context = TextContext::default();

    let gas_limit = 10000;

    let mut consensus_parameters = ConsensusParameters::default();
    consensus_parameters
        .set_tx_params(TxParameters::default().with_max_gas_per_tx(gas_limit));
    consensus_parameters.set_predicate_params(
        PredicateParameters::default().with_max_gas_per_predicate(gas_limit),
    );

    let coin = context
        .custom_predicate(
            AssetId::BASE,
            TEST_COIN_AMOUNT,
            // forever loop
            vec![op::jmp(RegId::ZERO)].into_iter().collect(),
            None,
        )
        .into_estimated(&consensus_parameters);

    let tx = TransactionBuilder::script(vec![], vec![])
        .script_gas_limit(GAS_LIMIT)
        .add_input(coin)
        .finalize_as_transaction();

    let err = check_tx(tx, &Default::default())
        .await
        .expect_err("Transaction should be err, got ok");

    assert!(
        err.to_string()
            .contains("PredicateVerificationFailed(OutOfGas)"),
        "unexpected error: {err}",
    )
}

#[tokio::test]
async fn predicate_that_returns_false_is_invalid() {
    let mut context = TextContext::default();
    let coin = context
        .custom_predicate(
            AssetId::BASE,
            TEST_COIN_AMOUNT,
            // forever loop
            vec![op::ret(RegId::ZERO)].into_iter().collect(),
            None,
        )
        .into_default_estimated();

    let tx = TransactionBuilder::script(vec![], vec![])
        .script_gas_limit(GAS_LIMIT)
        .add_input(coin)
        .finalize_as_transaction();

    let err = check_tx(tx, &Default::default())
        .await
        .expect_err("Transaction should be err, got ok");

    assert!(
        err.to_string().contains("PredicateVerificationFailed"),
        "unexpected error: {err}",
    )
}

#[tokio::test]
async fn insert_single__blob_tx_works() {
    let program = vec![123; 123];
    let tx = TransactionBuilder::blob(BlobBody {
        id: BlobId::compute(program.as_slice()),
        witness_index: 0,
    })
    .add_witness(program.into())
    .add_random_fee_input()
    .finalize_as_transaction();

    let config = Config {
        utxo_validation: false,
        ..Default::default()
    };
    let context = TextContext::default().config(config);
    let mut txpool = context.build();

    // Given
    let tx = check_unwrap_tx(tx, &txpool.config).await;
    let id = tx.id();

    // When
    let result = txpool.insert_single(tx);

    // Then
    let _ = result.expect("Should insert blob");
    assert!(txpool.by_hash.contains_key(&id));
}

#[tokio::test]
async fn insert_single__blob_tx_fails_if_blob_already_inserted_and_lower_tip() {
    let program = vec![123; 123];
    let blob_id = BlobId::compute(program.as_slice());
    let tx = TransactionBuilder::blob(BlobBody {
        id: blob_id,
        witness_index: 0,
    })
    .add_witness(program.clone().into())
    .add_random_fee_input()
    .finalize_as_transaction();

    let config = Config {
        utxo_validation: false,
        ..Default::default()
    };
    let context = TextContext::default().config(config);
    let mut txpool = context.build();
    let tx = check_unwrap_tx(tx, &txpool.config).await;

    // Given
    txpool.insert_single(tx).unwrap();
    let same_blob_tx = TransactionBuilder::blob(BlobBody {
        id: blob_id,
        witness_index: 1,
    })
    .add_random_fee_input()
    .add_witness(program.into())
    .finalize_as_transaction();
    let same_blob_tx = check_unwrap_tx(same_blob_tx, &txpool.config).await;

    // When
    let result = txpool.insert_single(same_blob_tx);

    // Then
    assert_eq!(result, Err(Error::NotInsertedCollisionBlobId(blob_id)));
}

#[tokio::test]
async fn insert_single__blob_tx_succeeds_if_blob_already_inserted_but_higher_tip() {
    let program = vec![123; 123];
    let blob_id = BlobId::compute(program.as_slice());
    let tx = TransactionBuilder::blob(BlobBody {
        id: blob_id,
        witness_index: 0,
    })
    .add_witness(program.clone().into())
    .add_random_fee_input()
    .finalize_as_transaction();

    let config = Config {
        utxo_validation: false,
        ..Default::default()
    };
    let context = TextContext::default().config(config);
    let mut txpool = context.build();
    let tx = check_unwrap_tx(tx, &txpool.config).await;

    // Given
    txpool.insert_single(tx).unwrap();
    let same_blob_tx = TransactionBuilder::blob(BlobBody {
        id: blob_id,
        witness_index: 1,
    })
    .add_random_fee_input()
    .add_witness(program.into())
    .tip(100)
    .max_fee_limit(100)
    .finalize_as_transaction();
    let same_blob_tx = check_unwrap_tx(same_blob_tx, &txpool.config).await;

    // When
    let result = txpool.insert_single(same_blob_tx);

    // Then
    let _ = result.expect("Should insert transaction with the same blob but higher tip");
}

#[tokio::test]
async fn insert_single__blob_tx_fails_if_blob_already_exists_in_database() {
    let program = vec![123; 123];
    let blob_id = BlobId::compute(program.as_slice());
    let tx = TransactionBuilder::blob(BlobBody {
        id: blob_id,
        witness_index: 0,
    })
    .add_witness(program.clone().into())
    .add_random_fee_input()
    .finalize_as_transaction();

    let config = Config {
        utxo_validation: false,
        ..Default::default()
    };
    let context = TextContext::default().config(config);
    let mut txpool = context.build();
    let tx = check_unwrap_tx(tx, &txpool.config).await;

    // Given
    txpool.database.0.insert_dummy_blob(blob_id);

    // When
    let result = txpool.insert_single(tx);

    // Then
    assert_eq!(result, Err(Error::NotInsertedBlobIdAlreadyTaken(blob_id)));
}

#[tokio::test]
async fn insert_inner__rejects_upgrade_tx_with_invalid_wasm() {
    let predicate = vec![op::ret(1)].into_iter().collect::<Vec<u8>>();
    let privileged_address = Input::predicate_owner(predicate.clone());

    let config = Config {
        utxo_validation: false,
        ..Default::default()
    };
    let context = TextContext::default()
        .config(config)
        .wasm_checker(MockWasmChecker {
            result: Err(WasmValidityError::NotValid),
        });
    let mut txpool = context.build();
    let gas_price_provider = MockTxPoolGasPrice::new(0);

    // Given
    let tx = TransactionBuilder::upgrade(UpgradePurpose::StateTransition {
        root: Bytes32::new([1; 32]),
    })
    .add_input(Input::coin_predicate(
        UtxoId::new(Bytes32::new([1; 32]), 0),
        privileged_address,
        1_000_000_000,
        AssetId::BASE,
        Default::default(),
        Default::default(),
        predicate,
        vec![],
    ))
    .finalize_as_transaction();
    let mut params = ConsensusParameters::default();
    params.set_privileged_address(privileged_address);
    let tx = check_single_tx(
        tx,
        Default::default(),
        false,
        &params,
        &gas_price_provider,
        MemoryInstance::new(),
    )
    .await
    .expect("Transaction should be checked");

    // When
    let result = txpool.insert_single(tx);

    // Then
    assert_eq!(result, Err(Error::NotInsertedInvalidWasm));
}
