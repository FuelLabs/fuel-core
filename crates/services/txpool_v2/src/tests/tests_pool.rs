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
        .extract_transactions_for_block(Constraints {
            minimal_gas_price: 0,
            max_gas: u64::MAX,
            maximum_txs: u16::MAX,
            maximum_block_size: u32::MAX,
            excluded_contracts: Default::default(),
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
        .extract_transactions_for_block(Constraints {
            minimal_gas_price: 0,
            max_gas: u64::MAX,
            maximum_txs: u16::MAX,
            maximum_block_size: u32::MAX,
            excluded_contracts: Default::default(),
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
        .extract_transactions_for_block(Constraints {
            minimal_gas_price: 0,
            max_gas: u64::MAX,
            maximum_txs: u16::MAX,
            maximum_block_size: u32::MAX,
            excluded_contracts: Default::default(),
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
        .extract_transactions_for_block(Constraints {
            minimal_gas_price: 0,
            max_gas: u64::MAX,
            maximum_txs: u16::MAX,
            maximum_block_size: u32::MAX,
            excluded_contracts: Default::default(),
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
        .extract_transactions_for_block(Constraints {
            minimal_gas_price: 0,
            max_gas: u64::MAX,
            maximum_txs: u16::MAX,
            maximum_block_size: u32::MAX,
            excluded_contracts: Default::default(),
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
                .extract_transactions_for_block(Constraints {
                    minimal_gas_price: 0,
                    max_gas: u64::MAX,
                    maximum_txs: u16::MAX,
                    maximum_block_size: u32::MAX,
                    excluded_contracts: Default::default(),
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
        .extract_transactions_for_block(Constraints {
            minimal_gas_price: 0,
            max_gas: u64::MAX,
            maximum_txs: u16::MAX,
            maximum_block_size: u32::MAX,
            excluded_contracts,
        });

    // Then
    assert_eq!(txs.len(), 3, "Should have 1 txs");
    assert_eq!(txs[2].id(), tx2_id, "First should be tx2");
    universe.assert_pool_integrity(&[tx1]);
}
