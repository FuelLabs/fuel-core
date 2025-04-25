#![allow(non_snake_case)]

use fuel_core::database::Database;
use fuel_core_storage::{
    StorageAsMut,
    tables::ConsensusParametersVersions,
    transactional::WriteTransaction,
};
use fuel_core_types::{
    blockchain::transaction::TransactionExt,
    fuel_asm::{
        RegId,
        op,
    },
    fuel_crypto::rand::{
        Rng,
        rngs::StdRng,
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
    services::block_producer::Components,
};
use rand::SeedableRng;

use crate::{
    config::Config,
    executor::Executor,
    ports::{
        Filter,
        TransactionFiltered,
    },
    tests::mocks::Consumer,
};

use super::mocks::{
    MockRelayer,
    MockTxPool,
};

fn basic_tx(rng: &mut StdRng) -> Transaction {
    TransactionBuilder::script(vec![], vec![])
        .add_input(given_coin_predicate(rng, 1000))
        .finalize_as_transaction()
}

fn empty_filter() -> Filter {
    Filter {
        excluded_contract_ids: Default::default(),
    }
}

fn given_coin_predicate(rng: &mut StdRng, amount: u64) -> Input {
    let predicate = op::ret(RegId::ONE).to_bytes().to_vec();
    let owner = Input::predicate_owner(&predicate);
    Input::coin_predicate(
        rng.r#gen(),
        owner,
        amount,
        Default::default(),
        Default::default(),
        Default::default(),
        predicate,
        vec![],
    )
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
    let mut rng = rand::rngs::StdRng::seed_from_u64(2322);

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
            Consumer::receive(&tx_pool_requests_receiver).respond_with(
                &[&tx2, &tx1, &tx4, &tx3],
                TransactionFiltered::NotFiltered,
            );
            // Request for thread 2
            Consumer::receive(&tx_pool_requests_receiver)
                .respond_with(&[], TransactionFiltered::NotFiltered);
        }
    });

    let expected_ids = [tx2, tx1, tx4, tx3]
        .map(|tx| tx.id(&ChainId::default()))
        .to_vec();
    let actual_ids = result
        .block
        .transactions()
        .iter()
        .map(|tx| tx.id(&ChainId::default()))
        .collect::<Vec<_>>();

    assert_eq!(expected_ids, actual_ids);
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
    let mut rng = rand::rngs::StdRng::seed_from_u64(2322);

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
        .add_input(given_coin_predicate(&mut rng, 1000))
        .add_output(Output::contract(0, Default::default(), Default::default()))
        .finalize_as_transaction();
    let short_tx: Transaction = TransactionBuilder::script(vec![], vec![])
        .add_input(given_coin_predicate(&mut rng, 1000))
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
            Consumer::receive(&tx_pool_requests_receiver)
                .assert_filter(&empty_filter())
                .respond_with(&[&long_tx], TransactionFiltered::NotFiltered);

            // Request for thread 2
            Consumer::receive(&tx_pool_requests_receiver)
                .assert_filter(&Filter {
                    excluded_contract_ids: vec![contract_id].into_iter().collect(),
                })
                .respond_with(&[], TransactionFiltered::Filtered);

            // Request for thread 1 again
            Consumer::receive(&tx_pool_requests_receiver)
                .assert_filter(&empty_filter())
                .respond_with(&[&short_tx], TransactionFiltered::NotFiltered);
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
    let mut rng = rand::rngs::StdRng::seed_from_u64(2322);

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
            .add_input(given_coin_predicate(&mut rng, 1000))
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
            .add_input(given_coin_predicate(&mut rng, 1000))
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
            .add_input(given_coin_predicate(&mut rng, 1000))
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
            Consumer::receive(&tx_pool_requests_receiver)
                .assert_filter(&empty_filter())
                .respond_with(&[&tx_contract_1], TransactionFiltered::NotFiltered);

            // Request for thread 2
            Consumer::receive(&tx_pool_requests_receiver)
                .assert_filter(&Filter {
                    excluded_contract_ids: vec![contract_id_1].into_iter().collect(),
                })
                .respond_with(&[&tx_contract_2], TransactionFiltered::NotFiltered);

            // Request for thread 1 again
            Consumer::receive(&tx_pool_requests_receiver)
                .assert_filter(&Filter {
                    excluded_contract_ids: vec![contract_id_2].into_iter().collect(),
                })
                .respond_with(&[], TransactionFiltered::Filtered);
            // Request for thread 1 or 2 again
            Consumer::receive(&tx_pool_requests_receiver)
                .assert_filter(&empty_filter())
                .assert_gas_limit_lt(
                    ConsensusParameters::default().block_gas_limit() - max_gas,
                )
                .respond_with(&[&tx_both_contracts], TransactionFiltered::NotFiltered);
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
    let mut rng = rand::rngs::StdRng::seed_from_u64(2322);
    let predicate = op::ret(RegId::ONE).to_bytes().to_vec();
    let owner = Input::predicate_owner(&predicate);

    // Given
    // TODO: Maybe need to make it last a bit longer to be sure it ends after second one
    let script = [op::add(RegId::ONE, 0x02, 0x03)];
    let script_bytes: Vec<u8> = script.iter().flat_map(|op| op.to_bytes()).collect();
    let tx1 = TransactionBuilder::script(script_bytes, vec![])
        .add_input(given_coin_predicate(&mut rng, 1000))
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
            Consumer::receive(&tx_pool_requests_receiver)
                .assert_filter(&empty_filter())
                .respond_with(&[&tx1], TransactionFiltered::NotFiltered);

            // Request for thread 2
            Consumer::receive(&tx_pool_requests_receiver)
                .assert_filter(&empty_filter())
                .respond_with(&[&tx2], TransactionFiltered::NotFiltered);
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
