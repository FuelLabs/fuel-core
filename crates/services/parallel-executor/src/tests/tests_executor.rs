#![allow(non_snake_case)]

use fuel_core_types::{
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
        Input,
        Transaction,
        TransactionBuilder,
        UniqueIdentifier,
    },
    fuel_types::ChainId,
    fuel_vm::checked_transaction::IntoChecked,
    services::block_producer::Components,
};
use rand::SeedableRng;

use crate::{
    config::Config,
    executor::Executor,
};

use super::mocks::{
    MockRelayer,
    MockStorage,
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

#[test]
#[ignore]
fn execute__simple_independent_transactions_sorted() {
    let executor = Executor::new(
        MockStorage,
        MockRelayer,
        Config {
            number_of_cores: std::num::NonZeroUsize::new(2)
                .expect("The value is not zero; qed"),
            executor_config: Default::default(),
        },
    );
    let (mut transactions_source, tx_pool_requests_receiver) = MockTxPool::new();
    let mut rng = rand::rngs::StdRng::seed_from_u64(2322u64);

    // Given
    let tx1: Transaction = basic_tx(&mut rng);
    let tx2: Transaction = basic_tx(&mut rng);
    let tx3: Transaction = basic_tx(&mut rng);
    let tx4: Transaction = basic_tx(&mut rng);
    transactions_source.register_transactions(vec![
        (
            tx1.clone()
                .into_checked_basic(0u32.into(), &ConsensusParameters::default())
                .unwrap()
                .into(),
            2,
        ),
        (
            tx2.clone()
                .into_checked_basic(0u32.into(), &ConsensusParameters::default())
                .unwrap()
                .into(),
            1,
        ),
        (
            tx3.clone()
                .into_checked_basic(0u32.into(), &ConsensusParameters::default())
                .unwrap()
                .into(),
            4,
        ),
        (
            tx4.clone()
                .into_checked_basic(0u32.into(), &ConsensusParameters::default())
                .unwrap()
                .into(),
            3,
        ),
    ]);

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

    dbg!(tx_pool_requests_receiver.recv().unwrap());

    // Then
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
fn execute__filter_contract_id_currently_executed() {}

#[test]
#[ignore]
fn execute__filter_contract_id_and_fetch_again_after() {}

#[test]
#[ignore]
fn execute__gas_left_updated_when_state_merges() {}

#[test]
#[ignore]
fn execute__utxo_ordering_kept() {}

#[test]
#[ignore]
fn execute__worst_case_merging() {}
