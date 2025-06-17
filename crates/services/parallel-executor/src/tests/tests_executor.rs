#![allow(non_snake_case)]

use fuel_core_storage::{
    Result as StorageResult,
    StorageAsMut,
    column::Column,
    kv_store::{
        KeyValueInspect,
        Value,
    },
    structured_storage::test::InMemoryStorage,
    tables::{
        Coins,
        ConsensusParametersVersions,
    },
    transactional::{
        AtomicView,
        Modifiable,
        StorageChanges,
        WriteTransaction,
    },
};
use fuel_core_types::{
    blockchain::transaction::TransactionExt,
    entities::coins::coin::Coin,
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
    fuel_vm::{
        Salt,
        checked_transaction::IntoChecked,
    },
    services::block_producer::Components,
};
use rand::SeedableRng;

use crate::{
    config::Config,
    executor::Executor,
    once_transaction_source::OnceTransactionsSource,
    ports::{
        Filter,
        TransactionFiltered,
    },
    tests::mocks::{
        MockPreconfirmationSender,
        MockRelayer,
        MockTransactionsSource,
        MockTxPoolResponse,
    },
};

#[derive(Clone, Debug, Default)]
struct Storage(pub InMemoryStorage<Column>);

impl KeyValueInspect for Storage {
    type Column = Column;

    fn get(&self, key: &[u8], column: Self::Column) -> StorageResult<Option<Value>> {
        self.0.get(key, column)
    }
}

impl AtomicView for Storage {
    type LatestView = Storage;

    fn latest_view(&self) -> StorageResult<Self::LatestView> {
        Ok(self.clone())
    }
}

impl Storage {
    fn merge_changes(&mut self, changes: StorageChanges) -> StorageResult<()> {
        match changes {
            StorageChanges::Changes(changes) => {
                self.0.commit_changes(changes)?;
            }
            StorageChanges::ChangesList(list) => {
                for change in list {
                    self.0.commit_changes(change)?;
                }
            }
        }
        Ok(())
    }
}

fn basic_tx(
    rng: &mut StdRng,
    database: &mut Storage,
    max_gas: Option<u64>,
) -> Transaction {
    let input = given_stored_coin_predicate(rng, 1000, database);
    let mut builder = TransactionBuilder::script(vec![], vec![]);
    builder.add_input(input);
    if let Some(gas) = max_gas {
        builder.script_gas_limit(gas);
    }
    builder.finalize_as_transaction()
}

fn empty_filter() -> Filter {
    Filter::new(Default::default())
}

fn given_stored_coin_predicate(
    rng: &mut StdRng,
    amount: u64,
    database: &mut Storage,
) -> Input {
    let predicate = op::ret(RegId::ONE).to_bytes().to_vec();
    let owner = Input::predicate_owner(&predicate);
    let utxo_id: UtxoId = rng.r#gen();
    let mut tx = database.0.write_transaction();
    tx.storage_as_mut::<Coins>()
        .insert(
            &utxo_id,
            &(Coin {
                utxo_id,
                owner,
                amount,
                asset_id: Default::default(),
                tx_pointer: Default::default(),
            }
            .compress()),
        )
        .unwrap();
    tx.commit().unwrap();
    Input::coin_predicate(
        utxo_id,
        owner,
        amount,
        Default::default(),
        Default::default(),
        10000,
        predicate,
        vec![],
    )
}

fn add_consensus_parameters(
    mut database: Storage,
    consensus_parameters: &ConsensusParameters,
) -> Storage {
    // Set the consensus parameters for the executor.
    let mut tx = database.0.write_transaction();
    tx.storage_as_mut::<ConsensusParametersVersions>()
        .insert(&0, consensus_parameters)
        .unwrap();
    tx.commit().unwrap();
    database
}

async fn contract_creation_changes(rng: &mut StdRng) -> (ContractId, StorageChanges) {
    let mut storage = Storage::default();
    storage = add_consensus_parameters(storage, &ConsensusParameters::default());
    let tx_creation = TransactionBuilder::create(
        Default::default(),
        Salt::new(rng.r#gen()),
        Default::default(),
    )
    .add_input(given_stored_coin_predicate(rng, 1000, &mut storage))
    .add_contract_created()
    .finalize_as_transaction();
    let contract_id = tx_creation
        .outputs()
        .first()
        .expect("Expected contract id")
        .contract_id()
        .cloned()
        .expect("Expected contract id");
    let mut executor = Executor::new(
        storage,
        MockRelayer,
        MockPreconfirmationSender,
        Config {
            number_of_cores: std::num::NonZeroUsize::new(2)
                .expect("The value is not zero; qed"),
        },
    );
    let res = executor
        .produce_without_commit_with_source(Components {
            header_to_produce: Default::default(),
            transactions_source: OnceTransactionsSource::new(
                vec![
                    tx_creation
                        .into_checked_basic(0u32.into(), &ConsensusParameters::default())
                        .unwrap()
                        .into(),
                ],
                0,
            ),
            coinbase_recipient: Default::default(),
            gas_price: 0,
        })
        .await
        .unwrap()
        .into_changes();
    (contract_id, res)
}

#[tokio::test]
async fn execute__simple_independent_transactions_sorted() {
    let mut rng = rand::rngs::StdRng::seed_from_u64(2322);
    let mut storage = Storage::default();
    storage = add_consensus_parameters(storage, &ConsensusParameters::default());

    // Given
    let tx1: Transaction = basic_tx(&mut rng, &mut storage, None);
    let tx2: Transaction = basic_tx(&mut rng, &mut storage, None);
    let tx3: Transaction = basic_tx(&mut rng, &mut storage, None);
    let tx4: Transaction = basic_tx(&mut rng, &mut storage, None);

    let mut executor: Executor<Storage, MockRelayer, MockPreconfirmationSender> =
        Executor::new(
            storage,
            MockRelayer,
            MockPreconfirmationSender,
            Config {
                number_of_cores: std::num::NonZeroUsize::new(2)
                    .expect("The value is not zero; qed"),
            },
        );
    let (transactions_source, mock_tx_pool) = MockTransactionsSource::new();

    // When
    let future = executor.produce_without_commit_with_source(Components {
        header_to_produce: Default::default(),
        transactions_source,
        coinbase_recipient: Default::default(),
        gas_price: 0,
    });

    // Request for a thread
    mock_tx_pool.push_response(MockTxPoolResponse::new(
        &[&tx2, &tx1, &tx4, &tx3],
        TransactionFiltered::NotFiltered,
    ));
    // Request for a second thread
    mock_tx_pool.push_response(MockTxPoolResponse::new(
        &[],
        TransactionFiltered::NotFiltered,
    ));

    // Then
    let result = future.await.unwrap().into_result();

    let expected_ids = [tx2, tx1, tx4, tx3]
        .map(|tx| tx.id(&ChainId::default()))
        .to_vec();
    let actual_ids = result
        .block
        .transactions()
        .iter()
        .map(|tx| tx.id(&ChainId::default()))
        .rev()
        .skip(1)
        .rev()
        .collect::<Vec<_>>();

    assert_eq!(expected_ids, actual_ids);
}

#[tokio::test]
async fn execute__filter_contract_id_currently_executed_and_fetch_after() {
    let mut rng = rand::rngs::StdRng::seed_from_u64(2322);
    let (contract_id, changes) = contract_creation_changes(&mut rng).await;
    let mut storage = Storage::default();
    storage.merge_changes(changes).unwrap();
    storage = add_consensus_parameters(storage, &ConsensusParameters::default());

    // Given
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
        .add_input(given_stored_coin_predicate(&mut rng, 1000, &mut storage))
        .add_output(Output::contract(0, Default::default(), Default::default()))
        .finalize_as_transaction();
    let short_tx: Transaction = TransactionBuilder::script(vec![], vec![])
        .add_input(given_stored_coin_predicate(&mut rng, 1000, &mut storage))
        .finalize_as_transaction();

    let mut executor: Executor<Storage, MockRelayer, MockPreconfirmationSender> =
        Executor::new(
            storage,
            MockRelayer,
            MockPreconfirmationSender,
            Config {
                number_of_cores: std::num::NonZeroUsize::new(2)
                    .expect("The value is not zero; qed"),
            },
        );
    let (transactions_source, mock_tx_pool) = MockTransactionsSource::new();

    // When
    let future = executor.produce_without_commit_with_source(Components {
        header_to_produce: Default::default(),
        transactions_source,
        coinbase_recipient: Default::default(),
        gas_price: 0,
    });

    // Request for a thread
    mock_tx_pool.push_response(
        MockTxPoolResponse::new(&[&long_tx], TransactionFiltered::NotFiltered)
            .assert_filter(empty_filter()),
    );

    // Request for a second thread
    mock_tx_pool.push_response(
        MockTxPoolResponse::new(&[], TransactionFiltered::Filtered)
            .assert_filter(Filter::new(vec![contract_id].into_iter().collect())),
    );

    // Request for one of the threads again that asked before
    mock_tx_pool.push_response(
        MockTxPoolResponse::new(&[&short_tx], TransactionFiltered::NotFiltered)
            .assert_filter(empty_filter()),
    );

    // Request for the other one of the threads again that asked before
    mock_tx_pool.push_response(MockTxPoolResponse::new(
        &[],
        TransactionFiltered::NotFiltered,
    ));

    // Then
    let _ = future.await.unwrap().into_result();
}

#[tokio::test]
async fn execute__gas_left_updated_when_state_merges() {
    let mut rng = rand::rngs::StdRng::seed_from_u64(2322);
    let (contract_id_1, changes_1) = contract_creation_changes(&mut rng).await;
    let (contract_id_2, changes_2) = contract_creation_changes(&mut rng).await;
    let mut storage = Storage::default();
    storage.merge_changes(changes_1).unwrap();
    storage.merge_changes(changes_2).unwrap();
    storage = add_consensus_parameters(storage, &ConsensusParameters::default());

    // Given
    let tx_contract_1: Transaction = TransactionBuilder::script(vec![], vec![])
        .add_input(Input::contract(
            rng.r#gen(),
            Default::default(),
            Default::default(),
            Default::default(),
            contract_id_1,
        ))
        .add_input(given_stored_coin_predicate(&mut rng, 1000, &mut storage))
        .add_output(Output::contract(0, Default::default(), Default::default()))
        .finalize_as_transaction();
    let max_gas = tx_contract_1
        .max_gas(&ConsensusParameters::default())
        .unwrap();
    let script = [
        op::movi(0x11, 32),
        op::aloc(0x11),
        op::movi(0x10, 0x00),
        op::cfe(0x10),
        op::k256(RegId::HP, RegId::ZERO, 0x10),
    ];
    let script_bytes: Vec<u8> = script.iter().flat_map(|op| op.to_bytes()).collect();
    let tx_contract_2: Transaction = TransactionBuilder::script(script_bytes, vec![])
        .add_input(Input::contract(
            rng.r#gen(),
            Default::default(),
            Default::default(),
            Default::default(),
            contract_id_2,
        ))
        .add_input(given_stored_coin_predicate(&mut rng, 1000, &mut storage))
        .add_output(Output::contract(0, Default::default(), Default::default()))
        .finalize_as_transaction();
    let tx_both_contracts: Transaction = TransactionBuilder::script(vec![], vec![])
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
        .add_input(given_stored_coin_predicate(&mut rng, 1000, &mut storage))
        .add_output(Output::contract(0, Default::default(), Default::default()))
        .add_output(Output::contract(1, Default::default(), Default::default()))
        .finalize_as_transaction();

    let mut executor: Executor<Storage, MockRelayer, MockPreconfirmationSender> =
        Executor::new(
            storage,
            MockRelayer,
            MockPreconfirmationSender,
            Config {
                number_of_cores: std::num::NonZeroUsize::new(2)
                    .expect("The value is not zero; qed"),
            },
        );
    let (transactions_source, mock_tx_pool) = MockTransactionsSource::new();

    // When
    let future = executor.produce_without_commit_with_source(Components {
        header_to_produce: Default::default(),
        transactions_source,
        coinbase_recipient: Default::default(),
        gas_price: 0,
    });

    // Request for one of the threads
    mock_tx_pool.push_response(
        MockTxPoolResponse::new(&[&tx_contract_1], TransactionFiltered::NotFiltered)
            .assert_filter(empty_filter()),
    );

    // Request for the other thread
    mock_tx_pool.push_response(
        MockTxPoolResponse::new(&[&tx_contract_2], TransactionFiltered::NotFiltered)
            .assert_filter(Filter::new(vec![contract_id_1].into_iter().collect())),
    );

    // Request for one of the threads again that asked before
    mock_tx_pool.push_response(
        MockTxPoolResponse::new(&[], TransactionFiltered::Filtered)
            .assert_filter(Filter::new(vec![contract_id_2].into_iter().collect())),
    );

    // Request for the other one of the threads again that asked before
    mock_tx_pool.push_response(
        MockTxPoolResponse::new(&[&tx_both_contracts], TransactionFiltered::NotFiltered)
            .assert_filter(empty_filter())
            .assert_gas_limit_lt(
                ConsensusParameters::default().block_gas_limit() - max_gas,
            ),
    );

    // Request for one of the threads again that asked before
    mock_tx_pool.push_response(MockTxPoolResponse::new(
        &[],
        TransactionFiltered::NotFiltered,
    ));

    // Then
    let _ = future.await.unwrap().into_result();
}

#[tokio::test]
async fn execute__utxo_ordering_kept() {
    let mut rng = rand::rngs::StdRng::seed_from_u64(2322);
    let predicate = op::ret(RegId::ONE).to_bytes().to_vec();
    let owner = Input::predicate_owner(&predicate);
    let mut storage = Storage::default();
    storage = add_consensus_parameters(storage, &ConsensusParameters::default());

    // Given
    let script = [op::add(RegId::ONE, 0x02, 0x03)];
    let script_bytes: Vec<u8> = script.iter().flat_map(|op| op.to_bytes()).collect();
    let tx1 = TransactionBuilder::script(script_bytes, vec![])
        .add_input(given_stored_coin_predicate(&mut rng, 1000, &mut storage))
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

    let mut executor: Executor<Storage, MockRelayer, MockPreconfirmationSender> =
        Executor::new(
            storage,
            MockRelayer,
            MockPreconfirmationSender,
            Config {
                number_of_cores: std::num::NonZeroUsize::new(2)
                    .expect("The value is not zero; qed"),
            },
        );
    let (transactions_source, mock_tx_pool) = MockTransactionsSource::new();

    // When
    let future = executor.produce_without_commit_with_source(Components {
        header_to_produce: Default::default(),
        transactions_source,
        coinbase_recipient: Default::default(),
        gas_price: 0,
    });

    // Request for one of the threads
    mock_tx_pool.push_response(
        MockTxPoolResponse::new(&[&tx1], TransactionFiltered::NotFiltered)
            .assert_filter(empty_filter()),
    );

    // Request for the other thread
    mock_tx_pool.push_response(
        MockTxPoolResponse::new(&[&tx2], TransactionFiltered::NotFiltered)
            .assert_filter(empty_filter()),
    );

    // Request for one of the threads again that asked before
    mock_tx_pool.push_response(
        MockTxPoolResponse::new(&[], TransactionFiltered::NotFiltered)
            .assert_filter(empty_filter()),
    );

    // Then
    let result = future.await.unwrap().into_result();

    let transactions = result.block.transactions();
    assert_eq!(transactions.len(), 3);
    assert_eq!(
        transactions[0].id(&ChainId::default()),
        tx1.id(&ChainId::default())
    );
    assert_eq!(
        transactions[1].id(&ChainId::default()),
        tx2.id(&ChainId::default())
    );
}

#[tokio::test]
async fn execute__utxo_resolved() {
    let mut rng = rand::rngs::StdRng::seed_from_u64(2322);
    let predicate = op::ret(RegId::ONE).to_bytes().to_vec();
    let owner = Input::predicate_owner(&predicate);
    let mut storage = Storage::default();
    storage = add_consensus_parameters(storage, &ConsensusParameters::default());

    // Given
    let script = [op::add(RegId::ONE, 0x02, 0x03)];
    let script_bytes: Vec<u8> = script.iter().flat_map(|op| op.to_bytes()).collect();
    let tx1 = TransactionBuilder::script(script_bytes, vec![])
        .add_input(given_stored_coin_predicate(&mut rng, 1000, &mut storage))
        .add_output(Output::change(owner, 0, Default::default()))
        .finalize_as_transaction();

    let mut executor = Executor::new(
        storage,
        MockRelayer,
        MockPreconfirmationSender,
        Config {
            number_of_cores: std::num::NonZeroUsize::new(2)
                .expect("The value is not zero; qed"),
        },
    );
    let (transactions_source, mock_tx_pool) = MockTransactionsSource::new();

    // When
    let future = executor.produce_without_commit_with_source(Components {
        header_to_produce: Default::default(),
        transactions_source,
        coinbase_recipient: Default::default(),
        gas_price: 0,
    });

    // Request for one of the threads
    mock_tx_pool.push_response(
        MockTxPoolResponse::new(&[&tx1], TransactionFiltered::NotFiltered)
            .assert_filter(empty_filter()),
    );

    // Request for the other thread
    mock_tx_pool.push_response(
        MockTxPoolResponse::new(&[], TransactionFiltered::NotFiltered)
            .assert_filter(empty_filter()),
    );

    // Then
    let result = future.await.unwrap().into_result();
    let transactions = result.block.transactions();
    assert_eq!(transactions.len(), 2);
    let output = transactions[0].outputs().into_owned()[0];
    assert_eq!(output.amount(), Some(1000));
}

// We use the overflow of gas to skip the transactions.
// TODO: This test can't be performed anymore now that we lower the gas ourself in the scheduler and so
// scheduler fails before the executor can skip the transaction.
#[tokio::test]
async fn execute__trigger_skipped_txs_fallback_mechanism() {
    let mut rng = rand::rngs::StdRng::seed_from_u64(2322);
    let mut storage = Storage::default();
    let mut consensus_parameters = ConsensusParameters::default();
    consensus_parameters.set_block_gas_limit(100000);
    storage = add_consensus_parameters(storage, &consensus_parameters);

    // Given
    let tx1: Transaction = basic_tx(&mut rng, &mut storage, Some(10));
    let tx2: Transaction = basic_tx(&mut rng, &mut storage, Some(10));
    let tx3: Transaction = basic_tx(&mut rng, &mut storage, Some(90000));
    let tx4: Transaction = basic_tx(&mut rng, &mut storage, Some(10));

    let mut executor: Executor<Storage, MockRelayer, MockPreconfirmationSender> =
        Executor::new(
            storage,
            MockRelayer,
            MockPreconfirmationSender,
            Config {
                number_of_cores: std::num::NonZeroUsize::new(3)
                    .expect("The value is not zero; qed"),
            },
        );
    let (transactions_source, mock_tx_pool) = MockTransactionsSource::new();

    // When
    let future = executor.produce_without_commit_with_source(Components {
        header_to_produce: Default::default(),
        transactions_source,
        coinbase_recipient: Default::default(),
        gas_price: 0,
    });

    // Request for a thread
    mock_tx_pool.push_response(
        MockTxPoolResponse::new(&[&tx1], TransactionFiltered::NotFiltered)
            .assert_filter(empty_filter()),
    );

    // Request for an other thread ( the second transaction is too large to fit in the block and will be skipped )
    mock_tx_pool.push_response(MockTxPoolResponse::new(
        &[&tx2, &tx3],
        TransactionFiltered::NotFiltered,
    ));

    // Request for an other thread
    mock_tx_pool.push_response(MockTxPoolResponse::new(
        &[&tx4],
        TransactionFiltered::NotFiltered,
    ));

    // Request for one of the threads again that asked before
    mock_tx_pool.push_response(MockTxPoolResponse::new(
        &[],
        TransactionFiltered::NotFiltered,
    ));

    // Then
    let result = future.await.unwrap().into_result();

    // 3 txs + mint tx (because tx2 has been skipped)
    assert_eq!(result.block.transactions().len(), 4);
}
