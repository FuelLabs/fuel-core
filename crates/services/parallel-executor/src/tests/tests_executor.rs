#![allow(non_snake_case)]
#![allow(clippy::arithmetic_side_effects)]

use std::time::Duration;

use crate::{
    config::{
        Config,
        WorkerCountPolicy,
    },
    executor::Executor,
    ports::{
        Filter,
        TransactionFiltered,
    },
    scheduler::fold_changes_in_canonical_order,
    tests::mocks::{
        MockPreconfirmationSender,
        MockRelayer,
        MockTransactionsSource,
        MockTxPoolResponse,
    },
};
use fuel_core_storage::{
    Result as StorageResult,
    StorageAsMut,
    column::Column,
    kv_store::{
        KeyValueInspect,
        StorageColumn,
        Value,
        WriteOperation,
    },
    structured_storage::test::InMemoryStorage,
    tables::{
        Coins,
        ConsensusParametersVersions,
    },
    transactional::{
        AtomicView,
        Changes,
        Modifiable,
        ReferenceBytesKey,
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
        Buildable,
        Chargeable,
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
        SecretKey,
    },
    services::block_producer::Components,
};
use rand::SeedableRng;
use tokio::time::Instant;

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

trait TransactionBuilderExt {
    fn add_stored_coin_input(
        &mut self,
        rng: &mut StdRng,
        storage: &mut Storage,
        amount: u64,
    ) -> &mut Self;
}

impl<Tx> TransactionBuilderExt for TransactionBuilder<Tx>
where
    Tx: Clone + Default + Chargeable + Buildable,
{
    fn add_stored_coin_input(
        &mut self,
        rng: &mut StdRng,
        storage: &mut Storage,
        amount: u64,
    ) -> &mut Self {
        let utxo_id: UtxoId = rng.r#gen();
        let secret_key = SecretKey::random(rng);
        let public_key = secret_key.public_key();
        let owner = Input::owner(&public_key);
        let mut tx = storage.0.write_transaction();
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
        self.add_unsigned_coin_input(
            secret_key,
            utxo_id,
            amount,
            Default::default(),
            Default::default(),
        );
        self
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

fn basic_tx(rng: &mut StdRng, database: &mut Storage) -> Transaction {
    let mut builder = TransactionBuilder::script(vec![], vec![]);
    builder.add_stored_coin_input(rng, database, 1000);
    builder.finalize_as_transaction()
}

fn empty_filter() -> Filter {
    Filter::new(Default::default())
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
    .add_stored_coin_input(rng, &mut storage, 1000)
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
            worker_count: std::num::NonZeroUsize::new(2)
                .expect("The value is not zero; qed"),
            worker_count_policy: crate::config::WorkerCountPolicy::StaticMax,
            metrics: false,
        },
    )
    .unwrap();

    let (source, mock_tx_pool) = MockTransactionsSource::new();

    mock_tx_pool.push_response(
        MockTxPoolResponse::new(&[&tx_creation], TransactionFiltered::NotFiltered)
            .assert_filter(empty_filter()),
    );

    let res = executor
        .produce_without_commit_with_source(
            Components {
                header_to_produce: Default::default(),
                transactions_source: source,
                coinbase_recipient: Default::default(),
                gas_price: 0,
            },
            Instant::now() + Duration::from_millis(300),
        )
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
    let tx1: Transaction = basic_tx(&mut rng, &mut storage);
    let tx2: Transaction = basic_tx(&mut rng, &mut storage);
    let tx3: Transaction = basic_tx(&mut rng, &mut storage);
    let tx4: Transaction = basic_tx(&mut rng, &mut storage);

    let mut executor: Executor<Storage, MockRelayer, MockPreconfirmationSender> =
        Executor::new(
            storage,
            MockRelayer,
            MockPreconfirmationSender,
            Config {
                worker_count: std::num::NonZeroUsize::new(2)
                    .expect("The value is not zero; qed"),
                worker_count_policy: crate::config::WorkerCountPolicy::StaticMax,
                metrics: false,
            },
        )
        .unwrap();
    let (transactions_source, mock_tx_pool) = MockTransactionsSource::new();

    // When
    let future = executor.produce_without_commit_with_source(
        Components {
            header_to_produce: Default::default(),
            transactions_source,
            coinbase_recipient: Default::default(),
            gas_price: 0,
        },
        Instant::now() + Duration::from_millis(300),
    );

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
async fn execute__when_dynamic_idle_policy_then_selection_uses_idle_worker_count() {
    let mut rng = rand::rngs::StdRng::seed_from_u64(2322);
    let mut storage = Storage::default();
    storage = add_consensus_parameters(storage, &ConsensusParameters::default());

    // given
    let script = [
        op::movi(0x11, 32),
        op::aloc(0x11),
        op::movi(0x10, 0x00),
        op::cfe(0x10),
        op::k256(RegId::HP, RegId::ZERO, 0x10),
    ];
    let script_bytes: Vec<u8> = script.iter().flat_map(|op| op.to_bytes()).collect();
    let long_tx = TransactionBuilder::script(script_bytes, vec![])
        .script_gas_limit(100_000)
        .add_stored_coin_input(&mut rng, &mut storage, 1000)
        .finalize_as_transaction();
    let mut executor: Executor<Storage, MockRelayer, MockPreconfirmationSender> =
        Executor::new(
            storage,
            MockRelayer,
            MockPreconfirmationSender,
            Config {
                worker_count: std::num::NonZeroUsize::new(2)
                    .expect("The value is not zero; qed"),
                worker_count_policy: WorkerCountPolicy::DynamicIdle,
                metrics: false,
            },
        )
        .unwrap();
    let (transactions_source, mock_tx_pool) = MockTransactionsSource::new();

    // when
    let future = executor.produce_without_commit_with_source(
        Components {
            header_to_produce: Default::default(),
            transactions_source,
            coinbase_recipient: Default::default(),
            gas_price: 0,
        },
        Instant::now() + Duration::from_millis(300),
    );
    mock_tx_pool.push_response(
        MockTxPoolResponse::new(&[&long_tx], TransactionFiltered::NotFiltered)
            .assert_selection_worker_count(2),
    );
    mock_tx_pool.push_response(
        MockTxPoolResponse::new(&[], TransactionFiltered::NotFiltered)
            .assert_selection_worker_count(1),
    );
    mock_tx_pool.push_response(MockTxPoolResponse::new(
        &[],
        TransactionFiltered::NotFiltered,
    ));

    // then
    let _ = future.await.unwrap();
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
        .add_stored_coin_input(&mut rng, &mut storage, 1000)
        .add_output(Output::contract(0, Default::default(), Default::default()))
        .finalize_as_transaction();
    let short_tx: Transaction = TransactionBuilder::script(vec![], vec![])
        .add_stored_coin_input(&mut rng, &mut storage, 1000)
        .finalize_as_transaction();

    let mut executor: Executor<Storage, MockRelayer, MockPreconfirmationSender> =
        Executor::new(
            storage,
            MockRelayer,
            MockPreconfirmationSender,
            Config {
                worker_count: std::num::NonZeroUsize::new(2)
                    .expect("The value is not zero; qed"),
                worker_count_policy: crate::config::WorkerCountPolicy::StaticMax,
                metrics: false,
            },
        )
        .unwrap();
    let (transactions_source, mock_tx_pool) = MockTransactionsSource::new();

    // When
    let future = executor.produce_without_commit_with_source(
        Components {
            header_to_produce: Default::default(),
            transactions_source,
            coinbase_recipient: Default::default(),
            gas_price: 0,
        },
        Instant::now() + Duration::from_millis(300),
    );

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
        .add_stored_coin_input(&mut rng, &mut storage, 1000)
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
        .script_gas_limit(100_000)
        .add_input(Input::contract(
            rng.r#gen(),
            Default::default(),
            Default::default(),
            Default::default(),
            contract_id_2,
        ))
        .add_stored_coin_input(&mut rng, &mut storage, 1000)
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
        .add_stored_coin_input(&mut rng, &mut storage, 1000)
        .add_output(Output::contract(0, Default::default(), Default::default()))
        .add_output(Output::contract(1, Default::default(), Default::default()))
        .finalize_as_transaction();

    let mut executor: Executor<Storage, MockRelayer, MockPreconfirmationSender> =
        Executor::new(
            storage,
            MockRelayer,
            MockPreconfirmationSender,
            Config {
                worker_count: std::num::NonZeroUsize::new(2)
                    .expect("The value is not zero; qed"),
                worker_count_policy: crate::config::WorkerCountPolicy::StaticMax,
                metrics: false,
            },
        )
        .unwrap();
    let (transactions_source, mock_tx_pool) = MockTransactionsSource::new();

    // When
    let future = executor.produce_without_commit_with_source(
        Components {
            header_to_produce: Default::default(),
            transactions_source,
            coinbase_recipient: Default::default(),
            gas_price: 0,
        },
        Instant::now() + Duration::from_millis(300),
    );

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

    std::thread::sleep(Duration::from_millis(100));

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
    let mut storage = Storage::default();
    storage = add_consensus_parameters(storage, &ConsensusParameters::default());
    let recipient_private_key = SecretKey::random(&mut rng);
    let recipient_public_key = recipient_private_key.public_key();
    let owner = Input::owner(&recipient_public_key);

    // Given
    let script = [op::add(RegId::ONE, 0x02, 0x03)];
    let script_bytes: Vec<u8> = script.iter().flat_map(|op| op.to_bytes()).collect();
    let tx1 = TransactionBuilder::script(script_bytes, vec![])
        .add_stored_coin_input(&mut rng, &mut storage, 1000)
        .add_output(Output::coin(owner, 1000, Default::default()))
        .finalize_as_transaction();

    let coin_utxo = UtxoId::new(tx1.id(&ChainId::default()), 0);
    let tx2 = TransactionBuilder::script(vec![], vec![])
        .add_unsigned_coin_input(
            recipient_private_key,
            coin_utxo,
            1000,
            Default::default(),
            Default::default(),
        )
        .add_output(Output::coin(owner, 1000, Default::default()))
        .finalize_as_transaction();

    let mut executor: Executor<Storage, MockRelayer, MockPreconfirmationSender> =
        Executor::new(
            storage,
            MockRelayer,
            MockPreconfirmationSender,
            Config {
                worker_count: std::num::NonZeroUsize::new(2)
                    .expect("The value is not zero; qed"),
                worker_count_policy: crate::config::WorkerCountPolicy::StaticMax,
                metrics: false,
            },
        )
        .unwrap();
    let (transactions_source, mock_tx_pool) = MockTransactionsSource::new();

    // When
    let future = executor.produce_without_commit_with_source(
        Components {
            header_to_produce: Default::default(),
            transactions_source,
            coinbase_recipient: Default::default(),
            gas_price: 0,
        },
        Instant::now() + Duration::from_millis(300),
    );

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

fuel_core_trace::enable_tracing!();

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
        .add_stored_coin_input(&mut rng, &mut storage, 1000)
        .add_output(Output::change(owner, 0, Default::default()))
        .finalize_as_transaction();

    let mut executor = Executor::new(
        storage,
        MockRelayer,
        MockPreconfirmationSender,
        Config {
            worker_count: std::num::NonZeroUsize::new(2)
                .expect("The value is not zero; qed"),
            worker_count_policy: crate::config::WorkerCountPolicy::StaticMax,
            metrics: false,
        },
    )
    .unwrap();
    let (transactions_source, mock_tx_pool) = MockTransactionsSource::new();

    // When
    let future = executor.produce_without_commit_with_source(
        Components {
            header_to_produce: Default::default(),
            transactions_source,
            coinbase_recipient: Default::default(),
            gas_price: 0,
        },
        Instant::now() + Duration::from_millis(300),
    );

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

// The fallback mechanism is triggered by a wrong predicate estimation
#[tokio::test]
async fn execute__trigger_skipped_txs_fallback_mechanism() {
    let mut rng = rand::rngs::StdRng::seed_from_u64(2322);
    let mut storage = Storage::default();
    let mut consensus_parameters = ConsensusParameters::default();
    consensus_parameters.set_block_gas_limit(100000);
    storage = add_consensus_parameters(storage, &consensus_parameters);
    let utxo_id: UtxoId = rng.r#gen();
    let code = [op::ret(RegId::ONE)];
    let code_bytes: Vec<u8> = code.iter().flat_map(|op| op.to_bytes()).collect();
    let owner = Input::predicate_owner(&code_bytes);
    let amount = 1000;
    let mut tx = storage.0.write_transaction();
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

    // Given
    let tx1: Transaction = basic_tx(&mut rng, &mut storage);
    let tx2: Transaction = basic_tx(&mut rng, &mut storage);

    let mut builder = TransactionBuilder::script(vec![], vec![]);
    builder.add_stored_coin_input(&mut rng, &mut storage, 1000);
    builder.add_input(Input::coin_predicate(
        utxo_id,
        owner,
        amount,
        Default::default(),
        Default::default(),
        Default::default(),
        code_bytes.clone(),
        vec![],
    ));
    let tx3 = builder.finalize_as_transaction();

    let tx4: Transaction = basic_tx(&mut rng, &mut storage);

    let mut executor: Executor<Storage, MockRelayer, MockPreconfirmationSender> =
        Executor::new(
            storage,
            MockRelayer,
            MockPreconfirmationSender,
            Config {
                worker_count: std::num::NonZeroUsize::new(3)
                    .expect("The value is not zero; qed"),
                worker_count_policy: crate::config::WorkerCountPolicy::StaticMax,
                metrics: false,
            },
        )
        .unwrap();
    let (transactions_source, mock_tx_pool) = MockTransactionsSource::new();

    // When
    let future = executor.produce_without_commit_with_source(
        Components {
            header_to_produce: Default::default(),
            transactions_source,
            coinbase_recipient: Default::default(),
            gas_price: 0,
        },
        Instant::now() + Duration::from_millis(300),
    );

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

// ---------------------------------------------------------------------------
// Audit-fix regression tests (coalescing / mint-on-merged-view / blob assert).
// ---------------------------------------------------------------------------

fn coins_key() -> ReferenceBytesKey {
    // 34 bytes = a compressed UtxoId key.
    vec![5u8; 34].into()
}

fn insert_op(byte: u8) -> WriteOperation {
    WriteOperation::Insert(Value::from(vec![byte]))
}

fn single_op_changes(
    column: Column,
    key: ReferenceBytesKey,
    op: WriteOperation,
) -> Changes {
    let mut changes = Changes::default();
    changes.entry(column.id()).or_default().insert(key, op);
    changes
}

// TEST 2 (fold semantics) — a DA-imported message (Insert in `da_changes`)
// consumed by exactly one in-block L2 tx (Remove) is LEGAL and folds, in
// canonical order (da first), to a net Remove. This is the harness-level proof
// for the DA-message scenario: driving a full relayer import through the mock
// parallel-executor harness would require a previous block + DA-height bump +
// a relayer returning a Message event + a matching message-input tx, none of
// which the current mock `TransactionsSource`/`MockRelayer` expose, so the fold
// function (the code that legitimises the split) is exercised directly.
#[test]
fn fold__da_imported_message_consumed_by_one_tx_folds_to_remove() {
    let key: ReferenceBytesKey = vec![3u8; 32].into();
    let da = single_op_changes(Column::Messages, key.clone(), insert_op(1));
    let batch = single_op_changes(Column::Messages, key.clone(), WriteOperation::Remove);

    let folded = fold_changes_in_canonical_order(vec![da, batch])
        .expect("Insert(da) then Remove(batch) is a legal create-then-spend fold");

    assert_eq!(
        folded
            .get(&Column::Messages.id())
            .and_then(|column| column.get(&key)),
        Some(&WriteOperation::Remove),
        "a consumed DA message must fold to a net Remove, matching the sequential map",
    );
}

// TEST 1 (fold semantics, unit level) — a coin created in batch i and spent in
// batch j > i folds (Insert then Remove, canonical order) to a net Remove.
#[test]
fn fold__coin_created_then_spent_across_batches_folds_to_remove() {
    let key = coins_key();
    let batch_i = single_op_changes(Column::Coins, key.clone(), insert_op(9));
    let batch_j = single_op_changes(Column::Coins, key.clone(), WriteOperation::Remove);

    let folded = fold_changes_in_canonical_order(vec![batch_i, batch_j]).unwrap();

    assert_eq!(
        folded.get(&Column::Coins.id()).and_then(|c| c.get(&key)),
        Some(&WriteOperation::Remove),
    );
}

// Genuine conflicts the ordering does NOT legitimise must still error.
#[test]
fn fold__two_inserts_of_the_same_coin_is_a_conflict() {
    let key = coins_key();
    let batch_i = single_op_changes(Column::Coins, key.clone(), insert_op(9));
    let batch_j = single_op_changes(Column::Coins, key.clone(), insert_op(10));

    assert!(
        fold_changes_in_canonical_order(vec![batch_i, batch_j]).is_err(),
        "two Inserts of the same UtxoId is impossible in a valid block and must error",
    );
}

#[test]
fn fold__double_remove_and_remove_then_insert_are_conflicts() {
    let key = coins_key();

    // Double spend of the same key.
    let a = single_op_changes(Column::Coins, key.clone(), WriteOperation::Remove);
    let b = single_op_changes(Column::Coins, key.clone(), WriteOperation::Remove);
    assert!(fold_changes_in_canonical_order(vec![a, b]).is_err());

    // Spend-before-create ordering.
    let c = single_op_changes(Column::Coins, key.clone(), WriteOperation::Remove);
    let d = single_op_changes(Column::Coins, key.clone(), insert_op(9));
    assert!(fold_changes_in_canonical_order(vec![c, d]).is_err());
}
