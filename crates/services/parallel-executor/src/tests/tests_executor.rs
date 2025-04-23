#![allow(non_snake_case)]

use fuel_core::database::Database;
use fuel_core_storage::{
    tables::ConsensusParametersVersions,
    transactional::WriteTransaction,
    StorageAsMut,
};
use fuel_core_types::{
    blockchain::transaction::TransactionExt,
    fuel_asm::{
        op,
        RegId,
    },
    fuel_crypto::rand::{
        rngs::StdRng,
        Rng,
    },
    fuel_tx::{
        ConsensusParameters,
        ContractId,
        Input,
        Output,
        Transaction,
        TransactionBuilder,
        UniqueIdentifier,
        UtxoId,
    },
    fuel_types::ChainId,
    fuel_vm::checked_transaction::IntoChecked,
    services::block_producer::Components,
};
use fuel_core_upgradable_executor::native_executor::ports::MaybeCheckedTransaction;
use rand::SeedableRng;

use crate::{
    config::Config,
    executor::Executor,
    ports::TransactionFiltered,
};

use super::mocks::{
    MockRelayer,
    MockTxPool,
};

fn basic_tx(rng: &mut StdRng) -> Transaction {
    let predicate = op::ret(RegId::ONE).to_bytes().to_vec();
    let owner = Input::predicate_owner(&predicate);

    TransactionBuilder::script(vec![], vec![])
        .add_input(Input::coin_predicate(
            rng.r#gen(),
            owner,
            1000,
            Default::default(),
            Default::default(),
            Default::default(),
            predicate.clone(),
            vec![],
        ))
        .finalize_as_transaction()
}

fn _add_consensus_parameters(
    mut database: Database,
    consensus_parameters: &ConsensusParameters,
) -> Database {
    // Set the consensus parameters for the executor.
    let mut tx = database.write_transaction();
    tx.storage_as_mut::<ConsensusParametersVersions>()
        .insert(&0, consensus_parameters)
        .unwrap();
    tx.commit().unwrap();
    database
}

#[test]
#[ignore]
fn execute__simple_independent_transactions_sorted() {
    let executor: Executor<Database, MockRelayer> = Executor::new(
        Database::default(),
        MockRelayer,
        Config {
            number_of_cores: std::num::NonZeroUsize::new(2)
                .expect("The value is not zero; qed"),
            executor_config: Default::default(),
        },
    );
    let (transactions_source, tx_pool_requests_receiver) = MockTxPool::new();
    let mut rng = rand::rngs::StdRng::seed_from_u64(2322u64);

    // Given
    let tx1: Transaction = basic_tx(&mut rng);
    let tx2: Transaction = basic_tx(&mut rng);
    let tx3: Transaction = basic_tx(&mut rng);
    let tx4: Transaction = basic_tx(&mut rng);

    // When
    let result = executor
        .produce_without_commit_with_source(Components {
            header_to_produce: Default::default(),
            transactions_source,
            coinbase_recipient: Default::default(),
            gas_price: 0,
        })
        .unwrap()
        .into_result();

    // Then
    std::thread::spawn({
        let tx1 = tx1.clone();
        let tx2 = tx2.clone();
        let tx3 = tx3.clone();
        let tx4 = tx4.clone();
        move || {
            // Request for thread 1
            let (_request, response_channel) = tx_pool_requests_receiver.recv().unwrap();
            response_channel
                .send((
                    vec![
                        MaybeCheckedTransaction::CheckedTransaction(
                            tx2.clone()
                                .into_checked_basic(
                                    0u32.into(),
                                    &ConsensusParameters::default(),
                                )
                                .unwrap()
                                .into(),
                            0,
                        ),
                        MaybeCheckedTransaction::CheckedTransaction(
                            tx1.clone()
                                .into_checked_basic(
                                    0u32.into(),
                                    &ConsensusParameters::default(),
                                )
                                .unwrap()
                                .into(),
                            0,
                        ),
                        MaybeCheckedTransaction::CheckedTransaction(
                            tx4.clone()
                                .into_checked_basic(
                                    0u32.into(),
                                    &ConsensusParameters::default(),
                                )
                                .unwrap()
                                .into(),
                            0,
                        ),
                        MaybeCheckedTransaction::CheckedTransaction(
                            tx3.clone()
                                .into_checked_basic(
                                    0u32.into(),
                                    &ConsensusParameters::default(),
                                )
                                .unwrap()
                                .into(),
                            0,
                        ),
                    ],
                    TransactionFiltered::NotFiltered,
                ))
                .unwrap();
        }
    });

    let transactions = result.block.transactions();
    assert_eq!(transactions.len(), 4);
    assert_eq!(
        transactions[0].id(&ChainId::default()),
        tx2.id(&ChainId::default())
    );
    assert_eq!(
        transactions[1].id(&ChainId::default()),
        tx1.id(&ChainId::default())
    );
    assert_eq!(
        transactions[2].id(&ChainId::default()),
        tx4.id(&ChainId::default())
    );
    assert_eq!(
        transactions[3].id(&ChainId::default()),
        tx3.id(&ChainId::default())
    );
}

#[test]
#[ignore]
fn execute__filter_contract_id_currently_executed_and_fetch_after() {
    let executor: Executor<Database, MockRelayer> = Executor::new(
        Database::default(),
        MockRelayer,
        Config {
            number_of_cores: std::num::NonZeroUsize::new(2)
                .expect("The value is not zero; qed"),
            executor_config: Default::default(),
        },
    );
    let (transactions_source, tx_pool_requests_receiver) = MockTxPool::new();
    let mut rng = rand::rngs::StdRng::seed_from_u64(2322u64);
    let predicate = op::ret(RegId::ONE).to_bytes().to_vec();
    let owner = Input::predicate_owner(&predicate);

    // Given
    let contract_id = ContractId::new([1; 32]);
    let script = [op::jmp(RegId::ZERO)];
    let script_bytes: Vec<u8> = script.iter().flat_map(|op| op.to_bytes()).collect();
    let long_tx: Transaction = TransactionBuilder::script(script_bytes.clone(), vec![])
        .add_input(Input::contract(
            rng.r#gen(),
            Default::default(),
            Default::default(),
            Default::default(),
            contract_id,
        ))
        .add_input(Input::coin_predicate(
            rng.r#gen(),
            owner,
            1000,
            Default::default(),
            Default::default(),
            Default::default(),
            predicate.clone(),
            vec![],
        ))
        .add_output(Output::contract(0, Default::default(), Default::default()))
        .finalize_as_transaction();
    let short_tx: Transaction = TransactionBuilder::script(vec![], vec![])
        .add_input(Input::coin_predicate(
            rng.r#gen(),
            owner,
            1000,
            Default::default(),
            Default::default(),
            Default::default(),
            predicate.clone(),
            vec![],
        ))
        .finalize_as_transaction();

    // When
    let _ = executor
        .produce_without_commit_with_source(Components {
            header_to_produce: Default::default(),
            transactions_source,
            coinbase_recipient: Default::default(),
            gas_price: 0,
        })
        .unwrap()
        .into_result();

    // Then
    std::thread::spawn({
        move || {
            // Request for thread 1
            let (pool_request_params, response_sender) =
                tx_pool_requests_receiver.recv().unwrap();
            assert_eq!(pool_request_params.filter.excluded_contract_ids.len(), 0);
            response_sender
                .send((
                    vec![MaybeCheckedTransaction::CheckedTransaction(
                        long_tx
                            .clone()
                            .into_checked_basic(
                                0u32.into(),
                                &ConsensusParameters::default(),
                            )
                            .unwrap()
                            .into(),
                        0,
                    )],
                    TransactionFiltered::NotFiltered,
                ))
                .unwrap();

            // Request for thread 2
            let (pool_request_params, response_sender) =
                tx_pool_requests_receiver.recv().unwrap();
            assert_eq!(pool_request_params.filter.excluded_contract_ids.len(), 1);
            assert_eq!(
                pool_request_params
                    .filter
                    .excluded_contract_ids
                    .get(&contract_id),
                Some(&contract_id)
            );
            response_sender
                .send((vec![], TransactionFiltered::Filtered))
                .unwrap();

            // Request for thread 1 again
            let (pool_request_params, response_sender) =
                tx_pool_requests_receiver.recv().unwrap();
            assert_eq!(pool_request_params.filter.excluded_contract_ids.len(), 0);
            response_sender
                .send((
                    vec![MaybeCheckedTransaction::CheckedTransaction(
                        short_tx
                            .clone()
                            .into_checked_basic(
                                0u32.into(),
                                &ConsensusParameters::default(),
                            )
                            .unwrap()
                            .into(),
                        0,
                    )],
                    TransactionFiltered::NotFiltered,
                ))
                .unwrap();
        }
    });
}

#[test]
#[ignore]
fn execute__gas_left_updated_when_state_merges() {
    let executor: Executor<Database, MockRelayer> = Executor::new(
        Database::default(),
        MockRelayer,
        Config {
            number_of_cores: std::num::NonZeroUsize::new(2)
                .expect("The value is not zero; qed"),
            executor_config: Default::default(),
        },
    );
    let (transactions_source, tx_pool_requests_receiver) = MockTxPool::new();
    let mut rng = rand::rngs::StdRng::seed_from_u64(2322u64);

    // Given
    let contract_id_1 = ContractId::new([1; 32]);
    let contract_id_2 = ContractId::new([2; 32]);
    let tx_contract_1: Transaction =
        TransactionBuilder::script(op::ret(RegId::ONE).to_bytes().to_vec(), vec![])
            .add_input(Input::contract(
                rng.r#gen(),
                Default::default(),
                Default::default(),
                Default::default(),
                contract_id_1,
            ))
            .add_input(Input::coin_predicate(
                rng.r#gen(),
                Default::default(),
                1000,
                Default::default(),
                Default::default(),
                Default::default(),
                op::ret(RegId::ONE).to_bytes().to_vec(),
                vec![],
            ))
            .add_output(Output::contract(0, Default::default(), Default::default()))
            .finalize_as_transaction();
    let max_gas = tx_contract_1
        .max_gas(&ConsensusParameters::default())
        .unwrap();
    let tx_contract_2: Transaction =
        TransactionBuilder::script(op::ret(RegId::ONE).to_bytes().to_vec(), vec![])
            .add_input(Input::contract(
                rng.r#gen(),
                Default::default(),
                Default::default(),
                Default::default(),
                contract_id_2,
            ))
            .add_input(Input::coin_predicate(
                rng.r#gen(),
                Default::default(),
                1000,
                Default::default(),
                Default::default(),
                Default::default(),
                op::ret(RegId::ONE).to_bytes().to_vec(),
                vec![],
            ))
            .add_output(Output::contract(0, Default::default(), Default::default()))
            .finalize_as_transaction();
    let tx_both_contracts: Transaction =
        TransactionBuilder::script(op::ret(RegId::ONE).to_bytes().to_vec(), vec![])
            .add_input(Input::contract(
                rng.r#gen(),
                Default::default(),
                Default::default(),
                Default::default(),
                contract_id_1,
            ))
            .add_input(Input::contract(
                rng.r#gen(),
                Default::default(),
                Default::default(),
                Default::default(),
                contract_id_2,
            ))
            .add_input(Input::coin_predicate(
                rng.r#gen(),
                Default::default(),
                1000,
                Default::default(),
                Default::default(),
                Default::default(),
                op::ret(RegId::ONE).to_bytes().to_vec(),
                vec![],
            ))
            .add_output(Output::contract(0, Default::default(), Default::default()))
            .add_output(Output::contract(1, Default::default(), Default::default()))
            .finalize_as_transaction();

    // When
    let _ = executor
        .produce_without_commit_with_source(Components {
            header_to_produce: Default::default(),
            transactions_source,
            coinbase_recipient: Default::default(),
            gas_price: 0,
        })
        .unwrap()
        .into_result();

    // Then
    // Request for thread 1
    std::thread::spawn({
        move || {
            let (pool_request_params, response_sender) =
                tx_pool_requests_receiver.recv().unwrap();
            assert_eq!(pool_request_params.filter.excluded_contract_ids.len(), 0);
            response_sender
                .send((
                    vec![MaybeCheckedTransaction::CheckedTransaction(
                        tx_contract_1
                            .clone()
                            .into_checked_basic(
                                0u32.into(),
                                &ConsensusParameters::default(),
                            )
                            .unwrap()
                            .into(),
                        0,
                    )],
                    TransactionFiltered::NotFiltered,
                ))
                .unwrap();

            // Request for thread 2
            let (pool_request_params, response_sender) =
                tx_pool_requests_receiver.recv().unwrap();
            assert_eq!(pool_request_params.filter.excluded_contract_ids.len(), 1);
            assert_eq!(
                pool_request_params
                    .filter
                    .excluded_contract_ids
                    .get(&contract_id_1),
                Some(&contract_id_1)
            );
            response_sender
                .send((
                    vec![MaybeCheckedTransaction::CheckedTransaction(
                        tx_contract_2
                            .clone()
                            .into_checked_basic(
                                0u32.into(),
                                &ConsensusParameters::default(),
                            )
                            .unwrap()
                            .into(),
                        0,
                    )],
                    TransactionFiltered::NotFiltered,
                ))
                .unwrap();
            // Request for thread 1 again
            let (pool_request_params, response_sender) =
                tx_pool_requests_receiver.recv().unwrap();
            assert_eq!(pool_request_params.filter.excluded_contract_ids.len(), 1);
            assert_eq!(
                pool_request_params
                    .filter
                    .excluded_contract_ids
                    .get(&contract_id_2),
                Some(&contract_id_2)
            );
            response_sender
                .send((vec![], TransactionFiltered::Filtered))
                .unwrap();
            // Request for thread 1 or 2 again
            let (pool_request_params, response_sender) =
                tx_pool_requests_receiver.recv().unwrap();
            // TODO: Maybe it's ConsensusParameters::default().block_gas_limit() / number_of_cores
            assert!(
                pool_request_params.gas_limit
                    > ConsensusParameters::default().block_gas_limit() - max_gas
            );
            assert_eq!(pool_request_params.filter.excluded_contract_ids.len(), 0);
            response_sender
                .send((
                    vec![MaybeCheckedTransaction::CheckedTransaction(
                        tx_both_contracts
                            .clone()
                            .into_checked_basic(
                                0u32.into(),
                                &ConsensusParameters::default(),
                            )
                            .unwrap()
                            .into(),
                        0,
                    )],
                    TransactionFiltered::NotFiltered,
                ))
                .unwrap();
        }
    });
}

#[test]
#[ignore]
fn execute__utxo_ordering_kept() {
    let executor: Executor<Database, MockRelayer> = Executor::new(
        Database::default(),
        MockRelayer,
        Config {
            number_of_cores: std::num::NonZeroUsize::new(2)
                .expect("The value is not zero; qed"),
            executor_config: Default::default(),
        },
    );
    let (transactions_source, tx_pool_requests_receiver) = MockTxPool::new();
    let mut rng = rand::rngs::StdRng::seed_from_u64(2322u64);
    let predicate = op::ret(RegId::ONE).to_bytes().to_vec();
    let owner = Input::predicate_owner(&predicate);

    // Given
    // TODO: Maybe need to make it last a bit longer to be sure it ends after second one
    let script = [op::add(RegId::ONE, 0x02, 0x03)];
    let script_bytes: Vec<u8> = script.iter().flat_map(|op| op.to_bytes()).collect();
    let tx1 = TransactionBuilder::script(script_bytes, vec![])
        .add_input(Input::coin_predicate(
            rng.r#gen(),
            owner,
            1000,
            Default::default(),
            Default::default(),
            Default::default(),
            predicate.clone(),
            vec![],
        ))
        .add_output(Output::coin(owner, 1000, Default::default()))
        .finalize_as_transaction();
    let coin_utxo = UtxoId::new(tx1.id(&ChainId::default()), 0);
    let tx2 = TransactionBuilder::script(vec![], vec![])
        .add_input(Input::coin_predicate(
            coin_utxo,
            owner,
            1000,
            Default::default(),
            Default::default(),
            Default::default(),
            predicate.clone(),
            vec![],
        ))
        .add_output(Output::coin(owner, 1000, Default::default()))
        .finalize_as_transaction();

    // When
    let result = executor
        .produce_without_commit_with_source(Components {
            header_to_produce: Default::default(),
            transactions_source,
            coinbase_recipient: Default::default(),
            gas_price: 0,
        })
        .unwrap()
        .into_result();

    // Then
    std::thread::spawn({
        let tx1 = tx1.clone();
        let tx2 = tx2.clone();
        move || {
            // Request for thread 1
            let (pool_request_params, response_sender) =
                tx_pool_requests_receiver.recv().unwrap();
            assert_eq!(pool_request_params.filter.excluded_contract_ids.len(), 0);
            response_sender
                .send((
                    vec![MaybeCheckedTransaction::CheckedTransaction(
                        tx1.clone()
                            .into_checked_basic(
                                0u32.into(),
                                &ConsensusParameters::default(),
                            )
                            .unwrap()
                            .into(),
                        0,
                    )],
                    TransactionFiltered::NotFiltered,
                ))
                .unwrap();

            // Request for thread 2
            let (pool_request_params, response_sender) =
                tx_pool_requests_receiver.recv().unwrap();
            assert_eq!(pool_request_params.filter.excluded_contract_ids.len(), 0);
            response_sender
                .send((
                    vec![MaybeCheckedTransaction::CheckedTransaction(
                        tx2.clone()
                            .into_checked_basic(
                                0u32.into(),
                                &ConsensusParameters::default(),
                            )
                            .unwrap()
                            .into(),
                        0,
                    )],
                    TransactionFiltered::NotFiltered,
                ))
                .unwrap();
        }
    });

    let transactions = result.block.transactions();
    assert_eq!(transactions.len(), 2);
    assert_eq!(
        transactions[0].id(&ChainId::default()),
        tx1.id(&ChainId::default())
    );
    assert_eq!(
        transactions[1].id(&ChainId::default()),
        tx2.id(&ChainId::default())
    );
}
