use std::collections::HashSet;

use crate::{
    config::{
        Config,
        PoolLimits,
    },
    error::{
        BlacklistedError,
        CollisionReason,
        DependencyError,
        Error,
        InputValidationError,
    },
    ports::WasmValidityError,
    selection_algorithms::Constraints,
    tests::{
        mocks::MockWasmChecker,
        universe::{
            GAS_LIMIT,
            IntoEstimated,
            TEST_COIN_AMOUNT,
            TestPoolUniverse,
            create_contract_input,
            create_contract_output,
            create_message_predicate_from_message,
        },
    },
};
use fuel_core_types::{
    fuel_asm::{
        RegId,
        op,
    },
    fuel_tx::{
        Address,
        AssetId,
        BlobBody,
        BlobId,
        BlobIdExt,
        Bytes32,
        Chargeable,
        ConsensusParameters,
        Contract,
        ContractId,
        Input,
        Output,
        PanicReason,
        PredicateParameters,
        TransactionBuilder,
        TxParameters,
        UniqueIdentifier,
        UpgradePurpose,
        UtxoId,
        ValidityError,
        input::coin::CoinPredicate,
    },
    fuel_types::ChainId,
    fuel_vm::{
        PredicateVerificationFailed,
        checked_transaction::{
            CheckError,
            CheckedTransaction,
            IntoChecked,
        },
    },
};

#[test]
fn insert_one_tx_succeeds() {
    let mut universe = TestPoolUniverse::default();
    universe.build_pool();

    // Given
    let tx = universe.build_script_transaction(None, None, 0);

    // When
    let result = universe.verify_and_insert(tx.clone());

    // Then
    assert!(result.is_ok());
    let tx = result.unwrap();
    universe.assert_pool_integrity(&[tx]);
}

#[test]
fn insert__tx_with_blacklisted_utxo_id() {
    let mut universe = TestPoolUniverse::default();

    // Given
    let coin = universe.setup_coin().1;
    let utxo_id = *coin.utxo_id().unwrap();
    universe.config.black_list.coins.insert(utxo_id);
    universe.build_pool();
    let tx = universe.build_script_transaction(Some(vec![coin]), None, 0);

    // When
    let err = universe.verify_and_insert(tx).unwrap_err();

    // Then
    assert!(
        matches!(err, Error::Blacklisted(BlacklistedError::BlacklistedUTXO(id)) if id == utxo_id)
    );
    universe.assert_pool_integrity(&[]);
}

#[test]
fn insert__tx_with_blacklisted_owner() {
    let mut universe = TestPoolUniverse::default();

    // Given
    let coin = universe.setup_coin().1;
    let owner_addr = *coin.input_owner().unwrap();
    universe.config.black_list.owners.insert(owner_addr);
    universe.build_pool();
    let tx = universe.build_script_transaction(Some(vec![coin]), None, 0);

    // When
    let err = universe.verify_and_insert(tx).unwrap_err();

    // Then
    assert!(
        matches!(err, Error::Blacklisted(BlacklistedError::BlacklistedOwner(id)) if id == owner_addr)
    );
    universe.assert_pool_integrity(&[]);
}

#[test]
fn insert__tx_with_blacklisted_contract() {
    let mut universe = TestPoolUniverse::default();
    let contract_id = Contract::EMPTY_CONTRACT_ID;

    // Given
    universe.config.black_list.contracts.insert(contract_id);
    universe.build_pool();
    let tx = universe.build_script_transaction(
        Some(vec![create_contract_input(
            Default::default(),
            Default::default(),
            contract_id,
        )]),
        Some(vec![Output::contract(
            0,
            Default::default(),
            Default::default(),
        )]),
        0,
    );

    // When
    let err = universe.verify_and_insert(tx).unwrap_err();

    // Then
    assert!(
        matches!(err, Error::Blacklisted(BlacklistedError::BlacklistedContract(id)) if id == contract_id)
    );
    universe.assert_pool_integrity(&[]);
}

#[test]
fn insert__tx_with_blacklisted_message() {
    let mut universe = TestPoolUniverse::default();

    // Given
    let (message, input) = create_message_predicate_from_message(5000, 0);
    let nonce = *message.nonce();
    universe.config.black_list.messages.insert(nonce);
    universe.build_pool();
    let tx = universe.build_script_transaction(Some(vec![input]), None, 0);

    // When
    let err = universe.verify_and_insert(tx).unwrap_err();

    // Then
    assert!(
        matches!(err, Error::Blacklisted(BlacklistedError::BlacklistedMessage(id)) if id == nonce)
    );
    universe.assert_pool_integrity(&[]);
}

#[test]
fn insert__tx2_succeeds_after_dependent_tx1() {
    let mut universe = TestPoolUniverse::default();
    universe.build_pool();

    // Given
    let (output, unset_input) = universe.create_output_and_input();
    let tx1 = universe.build_script_transaction(None, Some(vec![output]), 0);

    let input = unset_input.into_input(UtxoId::new(tx1.id(&ChainId::default()), 0));
    let tx2 = universe.build_script_transaction(Some(vec![input]), None, 0);

    // When
    let result1 = universe.verify_and_insert(tx1);
    let result2 = universe.verify_and_insert(tx2);

    // Then
    assert!(result1.is_ok());
    assert!(result2.is_ok());
    universe.assert_pool_integrity(&[result1.unwrap(), result2.unwrap()]);
}

#[test]
fn insert__tx2_collided_on_contract_id() {
    let mut universe = TestPoolUniverse::default();
    universe.build_pool();

    let contract_id = Contract::EMPTY_CONTRACT_ID;

    // contract creation tx
    let (_, gas_coin) = universe.setup_coin();
    let tx = TransactionBuilder::create(
        Default::default(),
        Default::default(),
        Default::default(),
    )
    .tip(10)
    .max_fee_limit(10)
    .add_input(gas_coin)
    .add_output(create_contract_output(contract_id))
    .finalize_as_transaction();

    let (_, gas_coin) = universe.setup_coin();

    // Given
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
    .add_output(create_contract_output(contract_id))
    .finalize_as_transaction();
    let tx = universe.verify_and_insert(tx).unwrap();

    // When
    let result2 = universe.verify_and_insert(tx_faulty);

    // Then
    let err = result2.unwrap_err();
    assert!(
        matches!(err, Error::Collided(CollisionReason::ContractCreation(id)) if id == contract_id)
    );
    universe.assert_pool_integrity(&[tx]);
}

#[test]
fn insert__tx_with_dependency_on_invalid_utxo_type() {
    let mut universe = TestPoolUniverse::default();
    universe.build_pool();
    let contract_id = Contract::EMPTY_CONTRACT_ID;

    let gas_coin = universe.setup_coin().1;
    let tx = TransactionBuilder::create(
        Default::default(),
        Default::default(),
        Default::default(),
    )
    .add_input(gas_coin)
    .add_output(create_contract_output(contract_id))
    .finalize_as_transaction();
    let utxo_id = UtxoId::new(tx.id(&Default::default()), 0);

    // Given
    // create a second transaction with utxo id referring to
    // the wrong type of utxo (contract instead of coin)
    let random_predicate =
        universe.random_predicate(AssetId::BASE, TEST_COIN_AMOUNT, Some(utxo_id));
    let tx_faulty =
        universe.build_script_transaction(Some(vec![random_predicate]), None, 0);
    let tx = universe.verify_and_insert(tx).unwrap();

    // When
    let result2 = universe.verify_and_insert(tx_faulty);

    // Then
    let err = result2.unwrap_err();

    assert!(
        matches!(err, Error::InputValidation(InputValidationError::UtxoNotFound(id)) if id == utxo_id)
    );
    universe.assert_pool_integrity(&[tx]);
}

#[test]
fn extract_transactions_for_block__revisits_deferred_complex_txs_in_same_block() {
    let mut universe = TestPoolUniverse::default();
    universe.build_pool();

    let contract_a = ContractId::from([1u8; 32]);
    let contract_b = ContractId::from([2u8; 32]);

    let simple_a = universe.build_script_transaction(
        Some(vec![create_contract_input(
            Default::default(),
            0,
            contract_a,
        )]),
        Some(vec![Output::contract(
            0,
            Default::default(),
            Default::default(),
        )]),
        10,
    );
    let simple_b = universe.build_script_transaction(
        Some(vec![create_contract_input(
            Default::default(),
            0,
            contract_b,
        )]),
        Some(vec![Output::contract(
            0,
            Default::default(),
            Default::default(),
        )]),
        10,
    );
    let complex_ab_1 = universe.build_script_transaction(
        Some(vec![
            create_contract_input(Default::default(), 0, contract_a),
            create_contract_input(Default::default(), 1, contract_b),
        ]),
        Some(vec![
            Output::contract(0, Default::default(), Default::default()),
            Output::contract(1, Default::default(), Default::default()),
        ]),
        20,
    );
    let complex_ab_2 = universe.build_script_transaction(
        Some(vec![
            create_contract_input(Default::default(), 0, contract_a),
            create_contract_input(Default::default(), 1, contract_b),
        ]),
        Some(vec![
            Output::contract(0, Default::default(), Default::default()),
            Output::contract(1, Default::default(), Default::default()),
        ]),
        20,
    );

    let simple_a = universe.verify_and_insert(simple_a).unwrap();
    let simple_b = universe.verify_and_insert(simple_b).unwrap();
    let complex_ab_1 = universe.verify_and_insert(complex_ab_1).unwrap();
    let complex_ab_2 = universe.verify_and_insert(complex_ab_2).unwrap();

    let pool = universe.get_pool();
    let (selected, _, _) =
        pool.write()
            .extract_transactions_for_block_with_anchors(&Constraints {
                minimal_gas_price: 0,
                max_gas: u64::MAX,
                maximum_txs: 10,
                maximum_block_size: u64::MAX,
                excluded_contracts: HashSet::new(),
                execution_worker_count: 13,
            });

    let selected_ids = selected.iter().map(|tx| tx.id()).collect::<HashSet<_>>();
    let expected_ids = [
        simple_a.id(),
        simple_b.id(),
        complex_ab_1.id(),
        complex_ab_2.id(),
    ]
    .into_iter()
    .collect::<HashSet<_>>();

    assert_eq!(selected_ids, expected_ids);
    universe.assert_pool_integrity(&[]);
}

#[test]
fn insert__already_known_tx_returns_error() {
    let mut universe = TestPoolUniverse::default().config(Config {
        utxo_validation: false,
        ..Default::default()
    });
    universe.build_pool();

    // Given
    let tx = universe.build_script_transaction(None, None, 0);
    let pool_tx = universe.verify_and_insert(tx.clone()).unwrap();

    // When
    let result2 = universe.verify_and_insert(tx.clone());

    // Then
    let err = result2.unwrap_err();
    assert!(
        matches!(err, Error::InputValidation(InputValidationError::DuplicateTxId(id)) if id == tx.id(&ChainId::default()))
    );
    universe.assert_pool_integrity(&[pool_tx]);
}

#[test]
fn insert__unknown_utxo_returns_error() {
    let mut universe = TestPoolUniverse::default();
    universe.build_pool();

    // Given
    let input = universe.random_predicate(AssetId::BASE, TEST_COIN_AMOUNT, None);
    let utxo_id = input.utxo_id().cloned().unwrap();
    let tx = universe.build_script_transaction(Some(vec![input]), None, 0);

    // When
    let result = universe.verify_and_insert(tx);

    // Then
    let err = result.unwrap_err();
    assert!(
        matches!(err, Error::InputValidation(InputValidationError::UtxoNotFound(id)) if id == utxo_id)
    );
    universe.assert_pool_integrity(&[]);
}

#[tokio::test]
async fn insert__higher_priced_tx_removes_lower_priced_tx() {
    let mut universe = TestPoolUniverse::default();
    universe.build_pool();

    // Given
    let common_coin = universe.setup_coin().1;
    let tx1 =
        universe.build_script_transaction(Some(vec![common_coin.clone()]), None, 10);
    let tx_id = tx1.id(&ChainId::default());
    let tx2 = universe.build_script_transaction(Some(vec![common_coin]), None, 20);

    // When
    universe.verify_and_insert(tx1).unwrap();
    let result = universe.verify_and_insert(tx2).unwrap();

    // Then
    universe
        .await_expected_tx_statuses_squeeze_out(vec![tx_id])
        .await;
    universe.assert_pool_integrity(&[result]);
}

#[test]
fn insert__colliding_dependent_and_underpriced_returns_error() {
    let mut universe = TestPoolUniverse::default();
    universe.build_pool();

    let (output, unset_input) = universe.create_output_and_input();
    let tx1 = universe.build_script_transaction(None, Some(vec![output]), 20);
    let utxo_id = UtxoId::new(tx1.id(&ChainId::default()), 0);
    let input = unset_input.into_input(utxo_id);

    // Given
    let tx2 = universe.build_script_transaction(Some(vec![input.clone()]), None, 20);
    let tx3 = universe.build_script_transaction(Some(vec![input]), None, 10);
    let tx1 = universe.verify_and_insert(tx1).unwrap();
    let tx2 = universe.verify_and_insert(tx2).unwrap();

    // When
    let result3 = universe.verify_and_insert(tx3);

    // Then
    let err = result3.unwrap_err();
    assert!(matches!(err, Error::Collided(CollisionReason::Utxo(id)) if id == utxo_id));
    universe.assert_pool_integrity(&[tx1, tx2]);
}

#[test]
fn insert_dependent_contract_creation() {
    let mut universe = TestPoolUniverse::default();
    universe.build_pool();
    let contract_id = Contract::EMPTY_CONTRACT_ID;

    // Given
    let (_, gas_funds) = universe.setup_coin();
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

    let tx2 = universe.build_script_transaction(
        Some(vec![create_contract_input(
            Default::default(),
            Default::default(),
            contract_id,
        )]),
        Some(vec![Output::contract(
            0,
            Default::default(),
            Default::default(),
        )]),
        10,
    );

    // When
    let result1 = universe.verify_and_insert(tx1);
    let result2 = universe.verify_and_insert(tx2);

    // Then
    assert!(result1.is_ok());
    assert!(result2.is_ok());
    universe.assert_pool_integrity(&[result1.unwrap(), result2.unwrap()]);
}

#[tokio::test]
async fn insert_more_priced_tx3_removes_tx1_and_dependent_tx2() {
    let mut universe = TestPoolUniverse::default();
    universe.build_pool();

    // Given
    let common_coin = universe.setup_coin().1;
    let (output, unset_input) = universe.create_output_and_input();

    let tx1 = universe.build_script_transaction(
        Some(vec![common_coin.clone()]),
        Some(vec![output]),
        10,
    );
    let tx1_id = tx1.id(&ChainId::default());
    let input = unset_input.into_input(UtxoId::new(tx1_id, 0));

    let tx2 = universe.build_script_transaction(Some(vec![input.clone()]), None, 10);
    let tx2_id = tx2.id(&ChainId::default());
    universe.verify_and_insert(tx1).unwrap();
    universe.verify_and_insert(tx2).unwrap();

    let tx3 = universe.build_script_transaction(Some(vec![common_coin]), None, 20);

    // When
    let result3 = universe.verify_and_insert(tx3);

    // Then
    let pool_tx = result3.unwrap();
    universe
        .await_expected_tx_statuses_squeeze_out(vec![tx1_id, tx2_id])
        .await;
    universe.assert_pool_integrity(&[pool_tx]);
}

#[tokio::test]
async fn insert_more_priced_tx2_removes_tx1_and_more_priced_tx3_removes_tx2() {
    let mut universe = TestPoolUniverse::default();
    universe.build_pool();

    // Given
    let common_coin = universe.setup_coin().1;

    let tx1 =
        universe.build_script_transaction(Some(vec![common_coin.clone()]), None, 10);
    let tx1_id = tx1.id(&ChainId::default());

    universe.verify_and_insert(tx1).unwrap();

    let tx2 =
        universe.build_script_transaction(Some(vec![common_coin.clone()]), None, 11);
    let tx2_id = tx2.id(&ChainId::default());

    let tx3 = universe.build_script_transaction(Some(vec![common_coin]), None, 12);

    // When
    let result2 = universe.verify_and_insert(tx2);
    let result3 = universe.verify_and_insert(tx3);

    // Then
    assert!(result2.is_ok());
    assert!(result3.is_ok());
    universe
        .await_expected_tx_statuses_squeeze_out(vec![tx1_id, tx2_id])
        .await;
    let pool_tx = result3.unwrap();
    universe.assert_pool_integrity(&[pool_tx]);
}

#[test]
fn insert__tx_limit_hit() {
    let mut universe = TestPoolUniverse::default().config(Config {
        pool_limits: PoolLimits {
            max_txs: 1,
            max_bytes_size: 1000000000,
            max_gas: 100_000_000_000,
        },
        ..Default::default()
    });
    universe.build_pool();

    // Given
    let tx1 = universe.build_script_transaction(None, None, 10);
    let tx2 = universe.build_script_transaction(None, None, 0);
    let pool_tx = universe.verify_and_insert(tx1).unwrap();

    // When
    let result2 = universe.verify_and_insert(tx2);

    // Then
    let err = result2.unwrap_err();
    assert!(matches!(err, Error::NotInsertedLimitHit));
    universe.assert_pool_integrity(&[pool_tx]);
}

#[test]
fn insert__tx_gas_limit() {
    // Given
    let mut universe = TestPoolUniverse::default();
    let tx1 = universe.build_script_transaction(None, None, 10);
    let checked_tx: CheckedTransaction = tx1
        .clone()
        .into_checked_basic(Default::default(), &ConsensusParameters::default())
        .unwrap()
        .into();
    let max_gas = match checked_tx {
        CheckedTransaction::Script(tx) => tx.metadata().max_gas,
        _ => panic!("Expected script transaction"),
    };
    let tx2 = universe.build_script_transaction(None, None, 0);
    universe = universe.config(Config {
        pool_limits: PoolLimits {
            max_txs: 10000,
            max_bytes_size: 1000000000,
            max_gas: max_gas + 10,
        },
        ..Default::default()
    });
    universe.build_pool();
    let pool_tx = universe.verify_and_insert(tx1).unwrap();

    // When
    let result2 = universe.verify_and_insert(tx2);

    // Then
    let err = result2.unwrap_err();
    assert!(matches!(err, Error::NotInsertedLimitHit));
    universe.assert_pool_integrity(&[pool_tx]);
}

#[test]
fn insert__tx_bytes_limit() {
    // Given
    let mut universe = TestPoolUniverse::default();
    let tx1 = universe.build_script_transaction(None, None, 10);
    let checked_tx: CheckedTransaction = tx1
        .clone()
        .into_checked_basic(Default::default(), &ConsensusParameters::default())
        .unwrap()
        .into();
    let max_bytes = match checked_tx {
        CheckedTransaction::Script(tx) => tx.transaction().metered_bytes_size(),
        _ => panic!("Expected script transaction"),
    };
    let tx2 = universe.build_script_transaction(None, None, 0);
    universe = universe.config(Config {
        pool_limits: PoolLimits {
            max_txs: 10000,
            max_bytes_size: max_bytes + 10,
            max_gas: 100_000_000_000,
        },
        ..Default::default()
    });
    universe.build_pool();
    let pool_tx = universe.verify_and_insert(tx1).unwrap();

    // When
    let result2 = universe.verify_and_insert(tx2);

    // Then
    let err = result2.unwrap_err();
    assert!(matches!(err, Error::NotInsertedLimitHit));
    universe.assert_pool_integrity(&[pool_tx]);
}

#[test]
fn insert__dependency_chain_length_hit() {
    let mut universe = TestPoolUniverse::default().config(Config {
        max_txs_chain_count: 2,
        ..Default::default()
    });
    universe.build_pool();

    // Given
    let (output, unset_input) = universe.create_output_and_input();
    let tx1 = universe.build_script_transaction(None, Some(vec![output]), 0);
    let input = unset_input.into_input(UtxoId::new(tx1.id(&Default::default()), 0));

    let (output, unset_input) = universe.create_output_and_input();
    let tx2 = universe.build_script_transaction(Some(vec![input]), Some(vec![output]), 0);
    let input = unset_input.into_input(UtxoId::new(tx2.id(&Default::default()), 0));

    let tx3 = universe.build_script_transaction(Some(vec![input]), None, 0);
    let tx1 = universe.verify_and_insert(tx1).unwrap();
    let tx2 = universe.verify_and_insert(tx2).unwrap();

    // When
    let result3 = universe.verify_and_insert(tx3);

    // Then
    let err = result3.unwrap_err();
    assert!(matches!(
        err,
        Error::Dependency(DependencyError::NotInsertedChainDependencyTooBig)
    ));
    universe.assert_pool_integrity(&[tx1, tx2]);
}

#[test]
fn get_sorted_out_tx1_2_3() {
    let mut universe = TestPoolUniverse::default();
    universe.build_pool();

    // Given
    let tx1 = universe.build_script_transaction(None, None, 10);
    let tx2 = universe.build_script_transaction(None, None, 9);
    let tx3 = universe.build_script_transaction(None, None, 20);

    let tx1_id = tx1.id(&ChainId::default());
    let tx2_id = tx2.id(&ChainId::default());
    let tx3_id = tx3.id(&ChainId::default());

    universe.verify_and_insert(tx1).unwrap();
    universe.verify_and_insert(tx2).unwrap();
    universe.verify_and_insert(tx3).unwrap();

    // When
    let txs = universe
        .get_pool()
        .write()
        .extract_transactions_for_block(&Constraints {
            minimal_gas_price: 0,
            max_gas: u64::MAX,
            maximum_txs: u32::MAX,
            maximum_block_size: u64::MAX,
            excluded_contracts: Default::default(),
            execution_worker_count: 1,
        });

    // Then
    assert_eq!(txs.len(), 3, "Should have 3 txs");
    assert_eq!(txs[0].id(), tx3_id, "First should be tx3");
    assert_eq!(txs[1].id(), tx1_id, "Second should be tx1");
    assert_eq!(txs[2].id(), tx2_id, "Third should be tx2");
    universe.assert_pool_integrity(&[]);
}

#[test]
fn get_sorted_out_tx_same_tips() {
    let mut universe = TestPoolUniverse::default();
    universe.build_pool();

    // Given
    let gas_coin = universe.setup_coin().1;
    let tx1 = TransactionBuilder::script(vec![], vec![])
        .tip(10)
        .max_fee_limit(10)
        .script_gas_limit(GAS_LIMIT)
        .add_input(gas_coin)
        .finalize_as_transaction();

    let (_, gas_coin) = universe.setup_coin();
    let tx2 = TransactionBuilder::script(vec![], vec![])
        .tip(10)
        .max_fee_limit(10)
        .script_gas_limit(GAS_LIMIT / 2)
        .add_input(gas_coin)
        .finalize_as_transaction();

    let (_, gas_coin) = universe.setup_coin();
    let tx3 = TransactionBuilder::script(vec![], vec![])
        .tip(10)
        .max_fee_limit(10)
        .script_gas_limit(GAS_LIMIT / 4)
        .add_input(gas_coin)
        .finalize_as_transaction();

    let tx1_id = tx1.id(&ChainId::default());
    let tx2_id = tx2.id(&ChainId::default());
    let tx3_id = tx3.id(&ChainId::default());

    universe.verify_and_insert(tx1).unwrap();
    universe.verify_and_insert(tx2).unwrap();
    universe.verify_and_insert(tx3).unwrap();

    // When
    let txs = universe
        .get_pool()
        .write()
        .extract_transactions_for_block(&Constraints {
            minimal_gas_price: 0,
            max_gas: u64::MAX,
            maximum_txs: u32::MAX,
            maximum_block_size: u64::MAX,
            excluded_contracts: Default::default(),
            execution_worker_count: 1,
        });

    // Then
    assert_eq!(txs.len(), 3, "Should have 3 txs");
    assert_eq!(txs[0].id(), tx3_id, "First should be tx3");
    assert_eq!(txs[1].id(), tx2_id, "Second should be tx2");
    assert_eq!(txs[2].id(), tx1_id, "Third should be tx1");
    universe.assert_pool_integrity(&[]);
}

#[test]
fn get_sorted_out_zero_tip() {
    let mut universe = TestPoolUniverse::default();
    universe.build_pool();

    // Given
    let gas_coin = universe.setup_coin().1;
    let tx1 = TransactionBuilder::script(vec![], vec![])
        .tip(10)
        .max_fee_limit(0)
        .script_gas_limit(GAS_LIMIT)
        .add_input(gas_coin)
        .finalize_as_transaction();

    let (_, gas_coin) = universe.setup_coin();
    let tx2 = TransactionBuilder::script(vec![], vec![])
        .tip(10)
        .max_fee_limit(0)
        .script_gas_limit(GAS_LIMIT / 2)
        .add_input(gas_coin)
        .finalize_as_transaction();

    let (_, gas_coin) = universe.setup_coin();
    let tx3 = TransactionBuilder::script(vec![], vec![])
        .tip(10)
        .max_fee_limit(0)
        .script_gas_limit(GAS_LIMIT / 4)
        .add_input(gas_coin)
        .finalize_as_transaction();

    let tx1_id = tx1.id(&ChainId::default());
    let tx2_id = tx2.id(&ChainId::default());
    let tx3_id = tx3.id(&ChainId::default());

    universe.verify_and_insert(tx1).unwrap();
    universe.verify_and_insert(tx2).unwrap();
    universe.verify_and_insert(tx3).unwrap();

    // When
    let txs = universe
        .get_pool()
        .write()
        .extract_transactions_for_block(&Constraints {
            minimal_gas_price: 0,
            max_gas: u64::MAX,
            maximum_txs: u32::MAX,
            maximum_block_size: u64::MAX,
            excluded_contracts: Default::default(),
            execution_worker_count: 1,
        });

    // Then
    assert_eq!(txs.len(), 3, "Should have 3 txs");
    assert_eq!(txs[0].id(), tx3_id, "First should be tx3");
    assert_eq!(txs[1].id(), tx2_id, "Second should be tx2");
    assert_eq!(txs[2].id(), tx1_id, "Third should be tx1");
    universe.assert_pool_integrity(&[]);
}

#[test]
fn get_sorted_out_tx_profitable_ratios() {
    let mut universe = TestPoolUniverse::default();
    universe.build_pool();

    // Given
    let gas_coin = universe.setup_coin().1;
    let tx1 = TransactionBuilder::script(vec![], vec![])
        .tip(4)
        .max_fee_limit(4)
        .script_gas_limit(GAS_LIMIT)
        .add_input(gas_coin)
        .finalize_as_transaction();

    let (_, gas_coin) = universe.setup_coin();
    let tx2 = TransactionBuilder::script(vec![], vec![])
        .tip(2)
        .max_fee_limit(2)
        .script_gas_limit(GAS_LIMIT / 10)
        .add_input(gas_coin)
        .finalize_as_transaction();

    let (_, gas_coin) = universe.setup_coin();
    let tx3 = TransactionBuilder::script(vec![], vec![])
        .tip(1)
        .max_fee_limit(1)
        .script_gas_limit(GAS_LIMIT / 100)
        .add_input(gas_coin)
        .finalize_as_transaction();

    let tx1_id = tx1.id(&ChainId::default());
    let tx2_id = tx2.id(&ChainId::default());
    let tx3_id = tx3.id(&ChainId::default());

    universe.verify_and_insert(tx1).unwrap();
    universe.verify_and_insert(tx2).unwrap();
    universe.verify_and_insert(tx3).unwrap();

    // When
    let txs = universe
        .get_pool()
        .write()
        .extract_transactions_for_block(&Constraints {
            minimal_gas_price: 0,
            max_gas: u64::MAX,
            maximum_txs: u32::MAX,
            maximum_block_size: u64::MAX,
            excluded_contracts: Default::default(),
            execution_worker_count: 1,
        });

    // Then
    assert_eq!(txs.len(), 3, "Should have 3 txs");
    assert_eq!(txs[0].id(), tx3_id, "First should be tx3");
    assert_eq!(txs[1].id(), tx2_id, "Second should be tx2");
    assert_eq!(txs[2].id(), tx1_id, "Third should be tx1");
    universe.assert_pool_integrity(&[]);
}

#[test]
fn get_sorted_out_tx_by_creation_instant() {
    let mut universe = TestPoolUniverse::default();
    universe.build_pool();

    // Given
    let tx1 = universe.build_script_transaction(None, None, 0);
    let tx2 = universe.build_script_transaction(None, None, 0);
    let tx3 = universe.build_script_transaction(None, None, 0);
    let tx4 = universe.build_script_transaction(None, None, 0);
    let tx1_id = tx1.id(&ChainId::default());
    let tx2_id = tx2.id(&ChainId::default());
    let tx3_id = tx3.id(&ChainId::default());
    let tx4_id = tx4.id(&ChainId::default());

    universe.verify_and_insert(tx1).unwrap();
    universe.verify_and_insert(tx2).unwrap();
    universe.verify_and_insert(tx3).unwrap();
    universe.verify_and_insert(tx4).unwrap();

    // When
    let txs = universe
        .get_pool()
        .write()
        .extract_transactions_for_block(&Constraints {
            minimal_gas_price: 0,
            max_gas: u64::MAX,
            maximum_txs: u32::MAX,
            maximum_block_size: u64::MAX,
            excluded_contracts: Default::default(),
            execution_worker_count: 1,
        });

    // Then
    // This order doesn't match the lexicographical order of the tx ids
    // and so it verifies that the txs are sorted by creation instant
    // The newest tx should be first
    assert_eq!(txs.len(), 4, "Should have 4 txs");
    assert_eq!(txs[0].id(), tx1_id, "First should be tx1");
    assert_eq!(txs[1].id(), tx2_id, "Second should be tx2");
    assert_eq!(txs[2].id(), tx3_id, "Third should be tx3");
    assert_eq!(txs[3].id(), tx4_id, "Fourth should be tx4");
    universe.assert_pool_integrity(&[]);
}

#[test]
fn insert_tx_at_least_min_gas_price() {
    // Given
    let gas_price = 10;
    let mut universe = TestPoolUniverse::default().config(Config {
        ..Default::default()
    });
    universe.build_pool();

    let tx = universe.build_script_transaction(None, None, gas_price);
    // When
    universe.verify_and_insert_with_gas_price(tx, gas_price)
    // Then
    .unwrap();
}

#[test]
fn insert__tx_below_min_gas_price() {
    // Given
    let gas_price = 1_000_000_000;
    let mut universe = TestPoolUniverse::default();
    universe.build_pool();

    let gas_coin = universe.setup_coin().1;
    let tx = TransactionBuilder::script(vec![], vec![])
        .tip(10)
        .max_fee_limit(10)
        .script_gas_limit(GAS_LIMIT)
        .add_input(gas_coin)
        .finalize_as_transaction();

    // When
    let err = universe
        .verify_and_insert_with_gas_price(tx, gas_price)
        .unwrap_err();

    // Then
    assert!(matches!(err, Error::InsufficientMaxFee { .. }));
    universe.assert_pool_integrity(&[]);
}

#[test]
fn insert_tx_when_input_message_id_exists_in_db() {
    let mut universe = TestPoolUniverse::default();
    universe.build_pool();

    // Given
    let (message, input) = create_message_predicate_from_message(5000, 0);
    universe.database_mut().insert_message(message);
    let tx = universe.build_script_transaction(Some(vec![input]), None, 0);

    // When
    let pool_tx = universe.verify_and_insert(tx)
    // Then
    .unwrap();
    universe.assert_pool_integrity(&[pool_tx]);
}

#[test]
fn insert__tx_when_input_message_id_do_not_exists_in_db() {
    let mut universe = TestPoolUniverse::default();
    universe.build_pool();

    // Given
    let (message, input) = create_message_predicate_from_message(5000, 0);
    let tx = universe.build_script_transaction(Some(vec![input]), None, 0);

    // When
    let err = universe.verify_and_insert(tx).unwrap_err();

    // Then
    assert!(matches!(
        err,
        Error::InputValidation(InputValidationError::NotInsertedInputMessageUnknown(msg_id)) if msg_id == *message.id()
    ));
    universe.assert_pool_integrity(&[]);
}

#[test]
fn insert__tx_tip_lower_than_another_tx_with_same_message_id() {
    let mut universe = TestPoolUniverse::default();
    universe.build_pool();

    let tip_high = 2u64;
    let tip_low = 1u64;
    let (message, conflicting_message_input) =
        create_message_predicate_from_message(10_000, 0);
    universe.database_mut().insert_message(message.clone());

    // Given
    let tx_high = universe.build_script_transaction(
        Some(vec![conflicting_message_input.clone()]),
        None,
        tip_high,
    );
    let tx_low = universe.build_script_transaction(
        Some(vec![conflicting_message_input]),
        None,
        tip_low,
    );

    // When
    let pool_tx = universe.verify_and_insert(tx_high).unwrap();
    let err = universe.verify_and_insert(tx_low).unwrap_err();

    // Then
    assert!(
        matches!(err, Error::Collided(CollisionReason::Message(msg_id)) if msg_id == *message.id())
    );
    universe.assert_pool_integrity(&[pool_tx]);
}

#[tokio::test]
async fn insert_tx_tip_higher_than_another_tx_with_same_message_id() {
    let mut universe = TestPoolUniverse::default();
    universe.build_pool();

    let tip_low = 1u64;
    let tip_high = 2u64;
    let (message, conflicting_message_input) =
        create_message_predicate_from_message(10_000, 0);
    universe.database_mut().insert_message(message.clone());

    // Given
    let tx_high = universe.build_script_transaction(
        Some(vec![conflicting_message_input.clone()]),
        None,
        tip_low,
    );
    let tx_high_id = tx_high.id(&ChainId::default());
    let tx_low = universe.build_script_transaction(
        Some(vec![conflicting_message_input]),
        None,
        tip_high,
    );

    // When
    let result1 = universe.verify_and_insert(tx_high);
    let result2 = universe.verify_and_insert(tx_low);

    // Then
    assert!(result1.is_ok());
    assert!(result2.is_ok());
    let pool_tx = result2.unwrap();
    universe
        .await_expected_tx_statuses_squeeze_out(vec![tx_high_id])
        .await;
    universe.assert_pool_integrity(&[pool_tx]);
}

#[test]
fn insert_again_message_after_squeeze_with_even_lower_tip() {
    // tx1 (message 1, message 2) tip 2
    // tx2 (message 1) tip 3
    //   squeezes tx1 with higher tip
    // tx3 (message 2) tip 1
    //   works since tx1 is no longer part of txpool state even though tip is less

    let mut universe = TestPoolUniverse::default();
    universe.build_pool();
    let (message_1, message_input_1) = create_message_predicate_from_message(10_000, 0);
    let (message_2, message_input_2) = create_message_predicate_from_message(20_000, 1);
    universe.database_mut().insert_message(message_1.clone());
    universe.database_mut().insert_message(message_2.clone());

    // Given
    let tx1 = universe.build_script_transaction(
        Some(vec![message_input_1.clone(), message_input_2.clone()]),
        None,
        2,
    );
    let tx2 = universe.build_script_transaction(Some(vec![message_input_1]), None, 3);
    let tx3 = universe.build_script_transaction(Some(vec![message_input_2]), None, 1);

    // When
    let result1 = universe.verify_and_insert(tx1);
    let result2 = universe.verify_and_insert(tx2);
    let result3 = universe.verify_and_insert(tx3);

    // Then
    assert!(result1.is_ok());
    assert!(result2.is_ok());
    assert!(result3.is_ok());
    universe.assert_pool_integrity(&[result2.unwrap(), result3.unwrap()]);
}

#[test]
fn insert__tx_with_predicates_incorrect_owner() {
    let mut universe = TestPoolUniverse::default().config(Config {
        utxo_validation: false,
        ..Default::default()
    });
    universe.build_pool();

    // Given
    let mut coin = universe.random_predicate(AssetId::BASE, TEST_COIN_AMOUNT, None);
    if let Input::CoinPredicate(CoinPredicate { owner, .. }) = &mut coin {
        *owner = Address::zeroed();
    }

    let tx = universe.build_script_transaction(Some(vec![coin]), None, 0);

    // When
    let err = universe.verify_and_insert(tx).unwrap_err();

    // Then
    assert!(matches!(
        err,
        Error::ConsensusValidity(CheckError::Validity(
            ValidityError::InputPredicateOwner { index: 0 }
        ))
    ));
    universe.assert_pool_integrity(&[]);
}

#[test]
fn insert__tx_with_predicate_without_enough_gas() {
    let mut universe = TestPoolUniverse::default().config(Config {
        utxo_validation: false,
        ..Default::default()
    });
    universe.build_pool();

    let gas_limit = 10000;

    // Given
    let mut consensus_parameters = ConsensusParameters::default();
    consensus_parameters
        .set_tx_params(TxParameters::default().with_max_gas_per_tx(gas_limit));
    consensus_parameters.set_predicate_params(
        PredicateParameters::default().with_max_gas_per_predicate(gas_limit),
    );

    let coin = universe
        .custom_predicate(
            AssetId::BASE,
            TEST_COIN_AMOUNT,
            // forever loop
            vec![op::jmp(RegId::ZERO)].into_iter().collect(),
            None,
        )
        .into_estimated(&consensus_parameters);

    let tx = universe.build_script_transaction(Some(vec![coin]), None, 0);

    // When
    let err = universe.verify_and_insert(tx).unwrap_err();

    // Then
    assert!(matches!(
        err,
        Error::ConsensusValidity(CheckError::PredicateVerificationFailed(
            PredicateVerificationFailed::OutOfGas { index: 0 }
        ))
    ));
    universe.assert_pool_integrity(&[]);
}

#[test]
fn insert__tx_with_predicate_that_returns_false() {
    let mut universe = TestPoolUniverse::default().config(Config {
        utxo_validation: false,
        ..Default::default()
    });
    universe.build_pool();

    // Given
    let coin = universe
        .custom_predicate(
            AssetId::BASE,
            TEST_COIN_AMOUNT,
            // ret false
            vec![op::ret(RegId::ZERO)].into_iter().collect(),
            None,
        )
        .into_default_estimated();

    let tx = universe.build_script_transaction(Some(vec![coin]), None, 0);

    // When
    let err = universe.verify_and_insert(tx).unwrap_err();

    // Then
    assert!(matches!(
        err,
        Error::ConsensusValidity(CheckError::PredicateVerificationFailed(
            PredicateVerificationFailed::Panic {
                index: 0,
                reason: PanicReason::PredicateReturnedNonOne
            }
        ))
    ));
    universe.assert_pool_integrity(&[]);
}

#[test]
fn insert_tx_with_blob() {
    let mut universe = TestPoolUniverse::default().config(Config {
        utxo_validation: false,
        ..Default::default()
    });
    universe.build_pool();

    // Given
    let program = vec![123; 123];
    let tx = TransactionBuilder::blob(BlobBody {
        id: BlobId::compute(program.as_slice()),
        witness_index: 0,
    })
    .add_witness(program.into())
    .add_fee_input()
    .finalize_as_transaction();

    // When
    let pool_tx = universe.verify_and_insert(tx)
    // Then
    .unwrap();
    universe.assert_pool_integrity(&[pool_tx]);
}

#[test]
fn insert__tx_with_blob_already_inserted_at_higher_tip() {
    let mut universe = TestPoolUniverse::default().config(Config {
        utxo_validation: false,
        ..Default::default()
    });
    universe.build_pool();

    // Given
    let program = vec![123; 123];
    let blob_id = BlobId::compute(program.as_slice());
    let tx = TransactionBuilder::blob(BlobBody {
        id: blob_id,
        witness_index: 0,
    })
    .add_witness(program.clone().into())
    .add_fee_input()
    .finalize_as_transaction();

    let pool_tx = universe.verify_and_insert(tx).unwrap();

    let same_blob_tx = TransactionBuilder::blob(BlobBody {
        id: blob_id,
        witness_index: 1,
    })
    .add_fee_input()
    .add_witness(program.into())
    .finalize_as_transaction();

    // When
    let err = universe.verify_and_insert(same_blob_tx).unwrap_err();

    // Then
    assert!(matches!(err, Error::Collided(CollisionReason::Blob(b)) if b == blob_id));
    universe.assert_pool_integrity(&[pool_tx]);
}

#[test]
fn insert_tx_with_blob_already_insert_at_lower_tip() {
    let mut universe = TestPoolUniverse::default().config(Config {
        utxo_validation: false,
        ..Default::default()
    });
    universe.build_pool();

    // Given
    let program = vec![123; 123];
    let blob_id = BlobId::compute(program.as_slice());
    let tx = TransactionBuilder::blob(BlobBody {
        id: blob_id,
        witness_index: 0,
    })
    .add_witness(program.clone().into())
    .add_fee_input()
    .finalize_as_transaction();

    universe.verify_and_insert(tx).unwrap();

    let same_blob_tx = TransactionBuilder::blob(BlobBody {
        id: blob_id,
        witness_index: 1,
    })
    .add_fee_input()
    .add_witness(program.into())
    .tip(100)
    .max_fee_limit(100)
    .finalize_as_transaction();

    // When
    let result = universe.verify_and_insert(same_blob_tx);

    // Then
    assert!(result.is_ok());
    universe.assert_pool_integrity(&[result.unwrap()]);
}

#[test]
fn verify_and_insert__when_dependent_tx_is_extracted_new_tx_still_accepted() {
    let mut universe = TestPoolUniverse::default();
    universe.build_pool();

    // Given
    let mut inputs = None;
    let (output_a, unset_input) = universe.create_output_and_input();
    let dependency_tx =
        universe.build_script_transaction(inputs.clone(), Some(vec![output_a]), 1);
    let mut pool_dependency_tx = universe.verify_and_insert(dependency_tx).unwrap();
    inputs = Some(vec![
        unset_input.into_input(UtxoId::new(pool_dependency_tx.id(), 0)),
    ]);

    // When
    for _ in 0..10 {
        let (output_a, new_unset_input) = universe.create_output_and_input();
        let dependent_tx =
            universe.build_script_transaction(inputs.clone(), Some(vec![output_a]), 1);
        let txs =
            universe
                .get_pool()
                .write()
                .extract_transactions_for_block(&Constraints {
                    minimal_gas_price: 0,
                    max_gas: u64::MAX,
                    maximum_txs: u32::MAX,
                    maximum_block_size: u64::MAX,
                    excluded_contracts: Default::default(),
                    execution_worker_count: 1,
                });
        assert_eq!(txs.len(), 1);
        assert_eq!(pool_dependency_tx.id(), txs[0].id());

        // Then
        pool_dependency_tx = universe.verify_and_insert(dependent_tx).unwrap();
        let input_a = new_unset_input.into_input(UtxoId::new(pool_dependency_tx.id(), 0));
        inputs = Some(vec![input_a.clone()]);
    }
}

#[test]
fn insert__tx_blob_already_in_db() {
    let mut universe = TestPoolUniverse::default().config(Config {
        utxo_validation: false,
        ..Default::default()
    });
    universe.build_pool();
    let program = vec![123; 123];
    let blob_id = BlobId::compute(program.as_slice());
    let tx = TransactionBuilder::blob(BlobBody {
        id: blob_id,
        witness_index: 0,
    })
    .add_witness(program.clone().into())
    .add_fee_input()
    .finalize_as_transaction();

    // Given
    universe.database_mut().insert_dummy_blob(blob_id);

    // When
    let err = universe.verify_and_insert(tx).unwrap_err();

    // Then
    assert!(matches!(
        err,
        Error::InputValidation(InputValidationError::NotInsertedBlobIdAlreadyTaken(b)) if b == blob_id
    ));
    universe.assert_pool_integrity(&[]);
}

#[test]
fn insert__dependent_on_blob() {
    let mut universe = TestPoolUniverse::default().config(Config {
        utxo_validation: false,
        ..Default::default()
    });
    universe.build_pool();
    let (output_a, unset_input) = universe.create_output_and_input();

    // Given
    let program = vec![123; 123];
    let blob_id = BlobId::compute(program.as_slice());
    let tx = TransactionBuilder::blob(BlobBody {
        id: blob_id,
        witness_index: 0,
    })
    .add_witness(program.clone().into())
    .add_fee_input()
    .add_output(output_a)
    .finalize_as_transaction();
    let tx_id = tx.id(&ChainId::default());

    let tx = universe.verify_and_insert(tx).unwrap();

    let input_a = unset_input.into_input(UtxoId::new(tx_id, 0));
    let dependent_tx = universe.build_script_transaction(Some(vec![input_a]), None, 1);

    // When
    universe.verify_and_insert(dependent_tx).unwrap_err();
    // Then
    universe.assert_pool_integrity(&[tx]);
}

#[test]
fn insert__if_tx3_depends_and_collides_with_tx2() {
    let mut universe = TestPoolUniverse::default();
    universe.build_pool();

    // tx1 {inputs: {}, outputs: {coinA}, tip: 1}
    let (output_a, unset_input) = universe.create_output_and_input();
    let tx1 = universe.build_script_transaction(None, Some(vec![output_a]), 1);
    // tx2 {inputs: {coinA}, outputs: {coinB}, tip: 1}
    let input_a = unset_input.into_input(UtxoId::new(tx1.id(&Default::default()), 0));
    let (output_b, unset_input) = universe.create_output_and_input();
    let tx2 = universe.build_script_transaction(
        Some(vec![input_a.clone()]),
        Some(vec![output_b]),
        1,
    );
    // Given
    // tx3 {inputs: {coinA, coinB}, outputs:{}, tip: 20}
    let input_b = unset_input.into_input(UtxoId::new(tx2.id(&Default::default()), 0));
    let tx1 = universe.verify_and_insert(tx1).unwrap();
    let tx2 = universe.verify_and_insert(tx2).unwrap();

    let tx3 = universe.build_script_transaction(Some(vec![input_a, input_b]), None, 20);

    // When
    let err = universe.verify_and_insert(tx3).unwrap_err();

    // Then
    assert!(matches!(
        err,
        Error::Dependency(DependencyError::DependentTransactionIsADiamondDeath)
    ));
    universe.assert_pool_integrity(&[tx1, tx2]);
}

#[test]
fn insert__tx_upgrade_with_invalid_wasm() {
    let mut universe = TestPoolUniverse::default().config(Config {
        utxo_validation: false,
        ..Default::default()
    });
    universe.build_pool();

    // Given
    let random_predicate =
        universe.random_predicate(AssetId::BASE, TEST_COIN_AMOUNT, None);
    let privileged_address = *random_predicate.input_owner().unwrap();
    let tx = TransactionBuilder::upgrade(UpgradePurpose::StateTransition {
        root: Bytes32::new([1; 32]),
    })
    .add_input(random_predicate)
    .finalize_as_transaction();
    let mut params = ConsensusParameters::default();
    params.set_privileged_address(privileged_address);

    // When
    let result = universe
        .verify_and_insert_with_consensus_params_wasm_checker(
            tx,
            params,
            MockWasmChecker::new(Err(WasmValidityError::NotEnabled)),
        )
        .unwrap_err();

    // Then
    assert!(matches!(
        result,
        Error::WasmValidity(WasmValidityError::NotEnabled)
    ));
    universe.assert_pool_integrity(&[]);
}

#[test]
fn extract__tx_with_excluded_contract() {
    let mut universe = TestPoolUniverse::default().config(Config {
        utxo_validation: false,
        ..Default::default()
    });
    universe.build_pool();

    // Given
    let (create_tx_1, excluded_contract) =
        universe.build_create_contract_transaction(vec![1, 2, 3]);
    let (create_tx_2, authorized_contract) =
        universe.build_create_contract_transaction(vec![4, 5, 6]);
    let tx1 = universe.build_script_transaction(
        Some(vec![Input::contract(
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            excluded_contract,
        )]),
        Some(vec![Output::contract(
            0,
            Default::default(),
            Default::default(),
        )]),
        0,
    );
    let tx2 = universe.build_script_transaction(
        Some(vec![Input::contract(
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            authorized_contract,
        )]),
        Some(vec![Output::contract(
            0,
            Default::default(),
            Default::default(),
        )]),
        0,
    );
    let mut excluded_contracts = HashSet::default();
    excluded_contracts.insert(excluded_contract);

    let tx2_id = tx2.id(&ChainId::default());

    universe.verify_and_insert(create_tx_1).unwrap();
    universe.verify_and_insert(create_tx_2).unwrap();
    let tx1 = universe.verify_and_insert(tx1).unwrap();
    universe.verify_and_insert(tx2).unwrap();

    // When
    let txs = universe
        .get_pool()
        .write()
        .extract_transactions_for_block(&Constraints {
            minimal_gas_price: 0,
            max_gas: u64::MAX,
            maximum_txs: u32::MAX,
            maximum_block_size: u64::MAX,
            excluded_contracts,
            execution_worker_count: 1,
        });

    // Then
    assert_eq!(txs.len(), 3, "Should have 1 txs");
    assert_eq!(txs[2].id(), tx2_id, "First should be tx2");
    universe.assert_pool_integrity(&[tx1]);
}

// ============================================================================
// Dependent-transaction promotion around block commit (core pool, lane
// scheduler OFF). These cover the three orderings of "child spends a Coin
// output of parent" vs the parent's extraction/commit, and document that the
// core dependency-graph promotion is sound in all three.
// ============================================================================

/// Case (a): parent and child both in the pool (child is a graph dependent).
/// The parent commits while still in the pool (e.g. the block came from
/// another producer). The child must be promoted to executable immediately.
#[test]
fn process_committed__parent_committed_from_pool_promotes_child() {
    let mut universe = TestPoolUniverse::default();
    universe.build_pool();

    // Given: tx2 depends on tx1's coin output; both in the pool.
    let (output, unset_input) = universe.create_output_and_input();
    let tx1 = universe.build_script_transaction(None, Some(vec![output]), 10);
    let tx1_id = tx1.id(&ChainId::default());
    universe.verify_and_insert(tx1).unwrap();
    let input = unset_input.into_input(UtxoId::new(tx1_id, 0));
    let tx2 = universe.build_script_transaction(Some(vec![input]), None, 20);
    let tx2_id = tx2.id(&ChainId::default());
    universe.verify_and_insert(tx2).unwrap();

    // When: the parent commits (imported block) while still in the pool.
    universe
        .get_pool()
        .write()
        .process_committed_transactions(std::iter::once(tx1_id));

    // Then: the child is promptly executable/extractable.
    let extracted = extract_one_batch(&universe, HashSet::new());
    let ids: Vec<_> = extracted.iter().map(|tx| tx.id()).collect();
    assert_eq!(ids, vec![tx2_id]);
    universe.assert_pool_integrity(&[]);
}

/// Case (b): the child arrives while the parent is EXTRACTED for a block that
/// is currently being produced (parent neither in the pool nor committed).
/// The child validates against the extracted outputs, becomes executable, and
/// must remain extractable after the parent's block commits.
#[test]
fn insert__child_mid_extraction_stays_extractable_after_parent_commit() {
    let mut universe = TestPoolUniverse::default();
    universe.build_pool();

    // Given: the parent is extracted for a block being produced.
    let (output, unset_input) = universe.create_output_and_input();
    let tx1 = universe.build_script_transaction(None, Some(vec![output]), 10);
    let tx1_id = tx1.id(&ChainId::default());
    universe.verify_and_insert(tx1).unwrap();
    let extracted = extract_one_batch(&universe, HashSet::new());
    assert_eq!(extracted.len(), 1);
    assert_eq!(extracted[0].id(), tx1_id);

    // When: the child arrives mid-production (spends the parent's coin output)
    // and then the parent's block commits.
    let input = unset_input.into_input(UtxoId::new(tx1_id, 0));
    let tx2 = universe.build_script_transaction(Some(vec![input]), None, 20);
    let tx2_id = tx2.id(&ChainId::default());
    universe.verify_and_insert(tx2).unwrap();
    {
        let pool = universe.get_pool();
        let mut pool = pool.write();
        // Mirrors `PoolWorker::process_block` for the committed parent.
        pool.process_committed_transactions(std::iter::once(tx1_id));
        pool.extracted_outputs.new_executed_transaction(&tx1_id);
    }

    // Then: the child is promptly extractable for the next block.
    let extracted = extract_one_batch(&universe, HashSet::new());
    let ids: Vec<_> = extracted.iter().map(|tx| tx.id()).collect();
    assert_eq!(ids, vec![tx2_id]);
    universe.assert_pool_integrity(&[]);
}

/// Case (c): the child arrives AFTER the parent's block committed (the coin is
/// already in the database). The child must be executable immediately.
#[test]
fn insert__child_after_parent_commit_is_immediately_extractable() {
    let mut universe = TestPoolUniverse::default();
    universe.build_pool();

    // Given: the parent was extracted and its block committed.
    let (output, unset_input) = universe.create_output_and_input();
    let tx1 = universe.build_script_transaction(None, Some(vec![output]), 10);
    let tx1_id = tx1.id(&ChainId::default());
    universe.verify_and_insert(tx1).unwrap();
    let extracted = extract_one_batch(&universe, HashSet::new());
    assert_eq!(extracted.len(), 1);
    {
        let pool = universe.get_pool();
        let mut pool = pool.write();
        pool.process_committed_transactions(std::iter::once(tx1_id));
        pool.extracted_outputs.new_executed_transaction(&tx1_id);
    }
    // The importer wrote the parent's coin output to the database.
    let input = unset_input.into_input(UtxoId::new(tx1_id, 0));
    {
        use fuel_core_types::entities::coins::coin::CompressedCoin;
        let mut coin = CompressedCoin::default();
        coin.set_owner(*input.input_owner().unwrap());
        coin.set_amount(1);
        coin.set_asset_id(AssetId::BASE);
        universe
            .database_mut()
            .data
            .lock()
            .unwrap()
            .coins
            .insert(UtxoId::new(tx1_id, 0), coin);
    }

    // When: the child arrives after the commit.
    let tx2 = universe.build_script_transaction(Some(vec![input]), None, 20);
    let tx2_id = tx2.id(&ChainId::default());
    universe.verify_and_insert(tx2).unwrap();

    // Then: it is immediately extractable.
    let extracted = extract_one_batch(&universe, HashSet::new());
    let ids: Vec<_> = extracted.iter().map(|tx| tx.id()).collect();
    assert_eq!(ids, vec![tx2_id]);
    universe.assert_pool_integrity(&[]);
}

// ============================================================================
// Lane scheduler integration tests (config flag `lane_scheduler`).
// ============================================================================

fn lane_config() -> Config {
    Config {
        lane_scheduler: true,
        // Contract-input transactions in these tests reference contracts that
        // are not present in the mock DB; skip UTXO/contract existence checks
        // (mirrors how contract-input txs are exercised elsewhere).
        utxo_validation: false,
        ..Default::default()
    }
}

/// Extract everything the lane scheduler is willing to give in one call, using a
/// non-binding (huge) budget and no in-flight locks.
fn extract_one_batch(
    universe: &TestPoolUniverse,
    excluded: HashSet<ContractId>,
) -> Vec<fuel_core_types::services::txpool::ArcPoolTx> {
    universe
        .get_pool()
        .write()
        .extract_transactions_for_block(&Constraints {
            minimal_gas_price: 0,
            max_gas: u64::MAX,
            maximum_txs: u32::MAX,
            maximum_block_size: u64::MAX,
            excluded_contracts: excluded,
            execution_worker_count: 1,
        })
}

/// Like [`extract_one_batch`] but also returns the lane-scheduler `BatchId`
/// assigned to the extracted batch (used to round-trip completion feedback).
fn extract_one_batch_with_id(
    universe: &TestPoolUniverse,
    excluded: HashSet<ContractId>,
) -> (
    Vec<fuel_core_types::services::txpool::ArcPoolTx>,
    Option<crate::lane_integration::BatchId>,
) {
    let (txs, _anchors, batch_id) = universe
        .get_pool()
        .write()
        .extract_transactions_for_block_with_anchors(&Constraints {
            minimal_gas_price: 0,
            max_gas: u64::MAX,
            maximum_txs: u32::MAX,
            maximum_block_size: u64::MAX,
            excluded_contracts: excluded,
            execution_worker_count: 1,
        });
    (txs, batch_id)
}

#[test]
fn lane_scheduler__batch_feedback_round_trips_completion() {
    use crate::lane_integration::BatchFeedback;

    let mut universe = TestPoolUniverse::default().config(lane_config());
    universe.build_pool();

    // Given: two independent transactions extracted as a single in-flight batch.
    let tx1 = universe.build_script_transaction(None, None, 10);
    let tx2 = universe.build_script_transaction(None, None, 20);
    universe.verify_and_insert(tx1).unwrap();
    universe.verify_and_insert(tx2).unwrap();

    let (batch, batch_id) = extract_one_batch_with_id(&universe, HashSet::new());
    assert_eq!(batch.len(), 2, "both txs should be dispatched in one batch");
    let batch_id = batch_id.expect("lane scheduler must assign a batch id");

    // The batch is dispatched but not yet reported complete → in flight.
    let in_flight_before = universe
        .get_pool()
        .read()
        .lane_scheduler
        .as_ref()
        .expect("lane scheduler enabled")
        .in_flight_batches();
    assert_eq!(
        in_flight_before, 1,
        "one dispatched batch awaiting feedback"
    );

    // When: the executor reports the batch complete (the executor→pool half of
    // the feedback loop), then the pool is asked again (feedback is drained onto
    // the next request, the scheduler's documented transport).
    universe
        .get_pool()
        .write()
        .lane_scheduler_feedback(BatchFeedback {
            batch_id,
            overhead_time: 5,
            execution_time: 100,
            completed: true,
        });
    let (drained, _) = extract_one_batch_with_id(&universe, HashSet::new());
    assert!(
        drained.is_empty(),
        "pool already drained; nothing new to give"
    );

    // Then: the completed batch is no longer in flight — the feedback landed.
    let in_flight_after = universe
        .get_pool()
        .read()
        .lane_scheduler
        .as_ref()
        .expect("lane scheduler enabled")
        .in_flight_batches();
    assert_eq!(
        in_flight_after, 0,
        "completion feedback must clear the in-flight batch"
    );
}

#[test]
fn lane_scheduler__committed_parent_makes_child_proposable_after_dropped_handle() {
    // Liveness regression: a parent is dispatched in a batch whose feedback
    // handle is DROPPED (executor crash / shutdown / any missed report). The
    // parent then commits on-chain and the pool processes the commit. The
    // parent's in-pool child must become proposable by the lane scheduler even
    // though no completion feedback ever arrived — the commit is an independent
    // completion path.
    let mut universe = TestPoolUniverse::default().config(lane_config());
    universe.build_pool();

    // Given: parent tx1 with a coin output, and child tx2 that spends it.
    let (output, unset_input) = universe.create_output_and_input();
    let tx1 = universe.build_script_transaction(None, Some(vec![output]), 10);
    let tx1_id = tx1.id(&ChainId::default());
    let input = unset_input.into_input(UtxoId::new(tx1_id, 0));
    let tx2 = universe.build_script_transaction(Some(vec![input]), None, 20);
    let tx2_id = tx2.id(&ChainId::default());
    universe.verify_and_insert(tx1).unwrap();
    universe.verify_and_insert(tx2).unwrap();

    // Extract: only the parent is proposable/extractable (the child still has an
    // in-pool dependency).
    let (batch, batch_id) = extract_one_batch_with_id(&universe, HashSet::new());
    let batch_ids: HashSet<_> = batch.iter().map(|tx| tx.id()).collect();
    assert_eq!(
        batch_ids,
        [tx1_id].into_iter().collect::<HashSet<_>>(),
        "only the parent is extractable in the first batch"
    );
    let _batch_id = batch_id.expect("lane scheduler must assign a batch id");

    // The feedback handle is DROPPED: we never call `lane_scheduler_feedback`.
    // (In production the executor owns the handle; a crash/shutdown drops it.)

    // Now the parent commits on-chain and the pool processes the commit. This
    // removes the parent and promotes the child to executable inside the pool.
    universe
        .get_pool()
        .write()
        .process_committed_transactions(std::iter::once(tx1_id));

    // Then: the child MUST now be proposable by the lane scheduler.
    let (batch2, _) = extract_one_batch_with_id(&universe, HashSet::new());
    let batch2_ids: HashSet<_> = batch2.iter().map(|tx| tx.id()).collect();
    assert_eq!(
        batch2_ids,
        [tx2_id].into_iter().collect::<HashSet<_>>(),
        "committed parent must make the child proposable even without feedback"
    );
    universe.assert_pool_integrity(&[]);
}

#[test]
fn lane_scheduler__unrelated_tx_stays_live_after_dropped_handle() {
    // A dispatched batch whose feedback handle is dropped must NOT wedge
    // unrelated (non-descendant) transactions: the scheduler keeps no persistent
    // lock table, so an in-flight-but-never-reported batch cannot block a fresh
    // independent tx.
    let mut universe = TestPoolUniverse::default().config(lane_config());
    universe.build_pool();

    // Given: one independent tx dispatched as a batch, handle dropped.
    let tx1 = universe.build_script_transaction(None, None, 10);
    universe.verify_and_insert(tx1).unwrap();
    let (batch, batch_id) = extract_one_batch_with_id(&universe, HashSet::new());
    assert_eq!(batch.len(), 1);
    let _ = batch_id.expect("lane scheduler must assign a batch id");
    // No feedback: the handle is dropped.

    // When: a fresh unrelated tx arrives.
    let tx2 = universe.build_script_transaction(None, None, 20);
    let tx2_id = tx2.id(&ChainId::default());
    universe.verify_and_insert(tx2).unwrap();

    // Then: it is immediately proposable.
    let (batch2, _) = extract_one_batch_with_id(&universe, HashSet::new());
    let batch2_ids: HashSet<_> = batch2.iter().map(|tx| tx.id()).collect();
    assert_eq!(
        batch2_ids,
        [tx2_id].into_iter().collect::<HashSet<_>>(),
        "an unrelated tx must stay live despite a dropped in-flight handle"
    );
}

fn writes_contract(
    tx: &fuel_core_types::services::txpool::ArcPoolTx,
    contract: ContractId,
) -> bool {
    crate::lane_integration::derive_contract_accesses(tx)
        .into_iter()
        .any(|(c, access)| {
            c == contract && access == crate::lane_integration::Access::Write
        })
}

#[test]
fn lane_scheduler__round_trips_independent_txs_when_enabled() {
    let mut universe = TestPoolUniverse::default().config(lane_config());
    universe.build_pool();

    // Given: three independent (no-contract) transactions.
    let tx1 = universe.build_script_transaction(None, None, 10);
    let tx2 = universe.build_script_transaction(None, None, 9);
    let tx3 = universe.build_script_transaction(None, None, 20);
    let expected: HashSet<_> = [
        tx1.id(&ChainId::default()),
        tx2.id(&ChainId::default()),
        tx3.id(&ChainId::default()),
    ]
    .into_iter()
    .collect();
    universe.verify_and_insert(tx1).unwrap();
    universe.verify_and_insert(tx2).unwrap();
    universe.verify_and_insert(tx3).unwrap();

    // When: draining the pool via the lane scheduler (loop across batches).
    let mut collected = HashSet::new();
    for _ in 0..10 {
        let batch = extract_one_batch(&universe, HashSet::new());
        if batch.is_empty() {
            break;
        }
        for tx in batch {
            collected.insert(tx.id());
        }
    }

    // Then: every transaction is selected exactly once and the pool empties.
    assert_eq!(collected, expected);
    universe.assert_pool_integrity(&[]);
}

#[test]
fn lane_scheduler__off_by_default_uses_classic_selection() {
    // Default config keeps the lane scheduler off — classic path returns txs.
    assert!(!Config::default().lane_scheduler);

    let mut universe = TestPoolUniverse::default();
    universe.build_pool();

    let tx1 = universe.build_script_transaction(None, None, 10);
    let tx2 = universe.build_script_transaction(None, None, 20);
    universe.verify_and_insert(tx1).unwrap();
    universe.verify_and_insert(tx2).unwrap();

    let txs = extract_one_batch(&universe, HashSet::new());
    assert_eq!(txs.len(), 2);
    universe.assert_pool_integrity(&[]);
}

#[test]
fn lane_scheduler__excluded_contract_writer_is_not_selected() {
    let mut universe = TestPoolUniverse::default().config(lane_config());
    universe.build_pool();

    let contract_a = ContractId::from([1u8; 32]);
    let contract_b = ContractId::from([2u8; 32]);
    // Register the contracts so their contract inputs validate.
    {
        let db = universe.database();
        let mut data = db.data.lock().unwrap();
        data.contracts.insert(contract_a, Contract::default());
        data.contracts.insert(contract_b, Contract::default());
    }

    // Writer of contract_a (contract input WITH matching output).
    let writes_a = universe.build_script_transaction(
        Some(vec![create_contract_input(
            Default::default(),
            0,
            contract_a,
        )]),
        Some(vec![Output::contract(
            0,
            Default::default(),
            Default::default(),
        )]),
        10,
    );
    // Writer of contract_b.
    let writes_b = universe.build_script_transaction(
        Some(vec![create_contract_input(
            Default::default(),
            0,
            contract_b,
        )]),
        Some(vec![Output::contract(
            0,
            Default::default(),
            Default::default(),
        )]),
        10,
    );
    let writes_a = universe.verify_and_insert(writes_a).unwrap();
    let writes_b = universe.verify_and_insert(writes_b).unwrap();

    // When: contract_a is already locked by an in-flight batch.
    let excluded = HashSet::from([contract_a]);
    let batch = extract_one_batch(&universe, excluded);

    // Then: the contract_a writer is withheld; only the contract_b writer runs.
    let ids: HashSet<_> = batch.iter().map(|tx| tx.id()).collect();
    assert!(
        ids.contains(&writes_b.id()),
        "contract_b writer should be selected"
    );
    assert!(
        !ids.contains(&writes_a.id()),
        "contract_a writer must not run concurrently with the in-flight lock"
    );
    for tx in &batch {
        assert!(
            !writes_contract(tx, contract_a),
            "no selected tx may write the excluded contract"
        );
    }
}

// NOTE (finding): reader-sharing is NOT reachable through a valid fuel
// transaction. A contract INPUT with no matching contract OUTPUT — the
// Read-derivation case — is rejected by consensus validity
// (`ValidityError::InputContractAssociatedOutputContract`): fuel-tx requires
// every contract input to have a matching contract output. The Read/Write
// derivation rule is therefore correct per the lane-scheduler spec, but every
// VALID fuel transaction yields only `Write` accesses today. The Read code path
// (and its concurrent reader-sharing) only becomes exercisable if a
// protocol-level read/write intent is introduced. The derivation itself is unit
// tested in `lane_integration::tests` (which builds the inputs/outputs directly,
// bypassing consensus validity).
