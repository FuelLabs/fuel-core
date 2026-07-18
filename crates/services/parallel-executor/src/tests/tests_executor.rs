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
    scheduler::{
        SchedulerExecutionResult,
        fold_changes_in_canonical_order,
    },
    tests::mocks::{
        MockPreconfirmationSender,
        MockRelayer,
        MockTransactionsSource,
        MockTxPoolResponse,
    },
};
use fuel_core_executor::executor::{
    ExecutionData,
    ExecutionInstance,
    ExecutionOptions,
    OnceTransactionsSource,
    TimeoutOnlyTxWaiter,
    TransparentPreconfirmationSender,
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
        FuelBlocks,
    },
    transactional::{
        AtomicView,
        Changes,
        ConflictPolicy,
        Modifiable,
        ReferenceBytesKey,
        StorageChanges,
        StorageTransaction,
        WriteTransaction,
    },
};
use fuel_core_types::{
    blockchain::{
        block::Block,
        header::PartialBlockHeader,
        transaction::TransactionExt,
    },
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
        Bytes32,
        Chargeable,
        ConsensusParameters,
        ContractId,
        Input,
        Output,
        Receipt,
        Transaction,
        TransactionBuilder,
        UniqueIdentifier,
        UtxoId,
    },
    fuel_types::ChainId,
    fuel_vm::{
        Salt,
        SecretKey,
        interpreter::MemoryInstance,
    },
    services::{
        block_producer::Components,
        executor::{
            TransactionExecutionResult,
            TransactionExecutionStatus,
        },
    },
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

    /// A properly signed coin input whose UTXO does NOT exist in the database
    /// (a "fake" coin). Only accepted with `utxo_validation = false`.
    fn add_fake_coin_input(&mut self, rng: &mut StdRng, amount: u64) -> &mut Self;
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

    fn add_fake_coin_input(&mut self, rng: &mut StdRng, amount: u64) -> &mut Self {
        let utxo_id: UtxoId = rng.r#gen();
        let secret_key = SecretKey::random(rng);
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
            utxo_validation: true,
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
                utxo_validation: true,
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
                utxo_validation: true,
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
                utxo_validation: true,
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
                utxo_validation: true,
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

    // Request for the other thread. tx_contract_1's batch (contract_id_1) is
    // in flight — but it is a fast batch, so by the time this pull happens it may
    // already have completed and freed contract_id_1. Both are valid exclusion
    // states; assert the pull is one of them rather than the wall-clock-dependent
    // exact value (this is the ~1/6 flake root cause).
    mock_tx_pool.push_response(
        MockTxPoolResponse::new(&[&tx_contract_2], TransactionFiltered::NotFiltered)
            .assert_filter_one_of(vec![
                Filter::new(vec![contract_id_1].into_iter().collect()),
                empty_filter(),
            ]),
    );

    // (Removed a `thread::sleep(100ms)` that ran BEFORE the future is awaited: the
    // block deadline is fixed at future-creation time, so the sleep only burned
    // ~1/3 of the 300ms window before the scheduler even started, making it race
    // to dispatch the final `tx_both_contracts` batch — the other half of the
    // flake. All responses are queued up-front regardless, so it synchronised
    // nothing.)

    // Request for one of the threads again that asked before. This pull fires
    // when the FIRST of the two batches completes and frees its worker — but
    // which one finishes first (the fast empty-script contract_1 batch or the
    // slow k256-loop contract_2 batch) is not fixed, and both can even be done.
    // So the excluded set is any of {}, {contract_1}, {contract_2} (at most one
    // batch still in flight); assert membership in that set rather than one
    // wall-clock-dependent value. This is the flake root cause.
    mock_tx_pool.push_response(
        MockTxPoolResponse::new(&[], TransactionFiltered::Filtered).assert_filter_one_of(
            vec![
                empty_filter(),
                Filter::new(vec![contract_id_1].into_iter().collect()),
                Filter::new(vec![contract_id_2].into_iter().collect()),
            ],
        ),
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
                utxo_validation: true,
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
            utxo_validation: true,
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
                utxo_validation: true,
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
// Package A regression tests — batch feedback fires on EVERY completion path
// (main loop, end-of-block drain, sequential fallback) and never leaks.
// ---------------------------------------------------------------------------

/// A script heavy enough that its batch is still executing on the worker runtime
/// when the (already-elapsed) block deadline breaks the scheduler's main loop —
/// so the batch is completed by the end-of-block drain (`wait_all_execution_tasks`)
/// rather than the main loop's `register_execution_result`.
fn heavy_tx(rng: &mut StdRng, storage: &mut Storage) -> Transaction {
    // A tight counted loop that burns many millions of iterations of real
    // interpreter work — tens of ms of wall-clock time, far longer than the
    // microseconds the scheduler's main loop takes to reach the deadline break,
    // so the batch is reliably still in flight when the loop breaks. Gas stays
    // under the 100M block/tx limit (and even gas-exhaustion would only revert,
    // never trigger the skipped-tx fallback).
    // `movi`'s immediate is 18-bit (< 262_144), so the counter is capped there;
    // a fat loop body (many cheap ops per iteration) supplies the rest of the
    // work.
    let mut ops = vec![op::movi(0x10, 10_000), op::movi(0x11, 0)];
    let loop_start = ops.len();
    ops.push(op::subi(0x10, 0x10, 1)); // counter -= 1
    for _ in 0..10 {
        ops.push(op::add(0x11, 0x11, 0x10)); // cheap busy-work
    }
    // Jump back over the whole body (subi + busy-work) while counter != 0.
    let back = (ops.len() - loop_start) as u16;
    ops.push(op::jnzb(0x10, RegId::ZERO, back));
    ops.push(op::ret(RegId::ONE));
    let script_bytes: Vec<u8> = ops.iter().flat_map(|op| op.to_bytes()).collect();
    TransactionBuilder::script(script_bytes, vec![])
        .script_gas_limit(50_000_000)
        .add_stored_coin_input(rng, storage, 1_000_000)
        .finalize_as_transaction()
}

// The end-of-block drain path (`wait_all_execution_tasks`) previously inserted a
// completed batch straight into `execution_results`, silently DROPPING its
// feedback handle (starving the lane scheduler's overhead EMA) and leaking its
// bookkeeping. With an already-elapsed deadline the heavy batch is still running
// when the main loop breaks, so it drains here. Whichever completion path runs,
// the handle must fire exactly once with `completed: true` and nothing must
// leak: `created == reports.len()`.
#[tokio::test]
async fn feedback__drain_path_reports_completed_true_and_does_not_leak() {
    let mut rng = rand::rngs::StdRng::seed_from_u64(2322);
    let mut storage = Storage::default();
    storage = add_consensus_parameters(storage, &ConsensusParameters::default());

    let tx = heavy_tx(&mut rng, &mut storage);

    let mut executor: Executor<Storage, MockRelayer, MockPreconfirmationSender> =
        Executor::new(
            storage,
            MockRelayer,
            MockPreconfirmationSender,
            Config {
                worker_count: std::num::NonZeroUsize::new(2).unwrap(),
                worker_count_policy: crate::config::WorkerCountPolicy::StaticMax,
                metrics: false,
                utxo_validation: true,
            },
        )
        .unwrap();
    let (transactions_source, mock_tx_pool, sink) =
        MockTransactionsSource::new_with_feedback();

    // Already-elapsed deadline: the main loop breaks while the heavy batch is
    // still in flight, forcing the drain path to complete it.
    let future = executor.produce_without_commit_with_source(
        Components {
            header_to_produce: Default::default(),
            transactions_source,
            coinbase_recipient: Default::default(),
            gas_price: 0,
        },
        Instant::now(),
    );
    mock_tx_pool.push_response(MockTxPoolResponse::new(
        &[&tx],
        TransactionFiltered::NotFiltered,
    ));

    let _ = future.await.unwrap().into_result();

    let reports = sink.reports();
    // Exactly one non-empty batch was dispatched, so exactly one handle exists.
    assert_eq!(
        sink.created(),
        1,
        "one non-empty batch should be dispatched"
    );
    // No dropped handle, no leak: every dispatched batch reported once.
    assert_eq!(
        reports.len(),
        sink.created(),
        "every dispatched batch's feedback handle must fire exactly once",
    );
    // The batch's results are kept, so it reports completion.
    assert!(
        reports.iter().all(|r| r.completed),
        "a kept batch must report completed=true, got {reports:?}",
    );
    // Real timings were measured (non-zero inner execution for a heavy batch).
    assert!(
        reports.iter().all(|r| r.execution_time > 0),
        "a completed batch must report a non-zero execution_time, got {reports:?}",
    );
    // Overhead is now reported too (batch preparation + per-contract handoff),
    // not left at zero — the feedback loop carries the batch's attributable
    // parallelization overhead.
    assert!(
        reports.iter().all(|r| r.overhead_time > 0),
        "a completed batch must report a non-zero overhead_time, got {reports:?}",
    );
}

// The sequential-fallback path (`sequential_fallback`) discards the parallel
// results of every in-flight batch and re-executes them serially. It previously
// dropped all those feedback handles and leaked their bookkeeping. Now each
// discarded batch must report `completed: false` (an overhead/timing signal for
// the EMA, but NOT a completion — the batch never committed), and no handle may
// leak. A bad predicate estimate deterministically triggers the fallback.
#[tokio::test]
async fn feedback__fallback_path_reports_completed_false_and_does_not_leak() {
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
                worker_count: std::num::NonZeroUsize::new(3).unwrap(),
                worker_count_policy: crate::config::WorkerCountPolicy::StaticMax,
                metrics: false,
                utxo_validation: true,
            },
        )
        .unwrap();
    let (transactions_source, mock_tx_pool, sink) =
        MockTransactionsSource::new_with_feedback();

    let future = executor.produce_without_commit_with_source(
        Components {
            header_to_produce: Default::default(),
            transactions_source,
            coinbase_recipient: Default::default(),
            gas_price: 0,
        },
        Instant::now() + Duration::from_millis(300),
    );
    mock_tx_pool.push_response(MockTxPoolResponse::new(
        &[&tx1],
        TransactionFiltered::NotFiltered,
    ));
    // tx3's predicate estimate is wrong, so this batch gets a skipped tx and
    // triggers the sequential fallback.
    mock_tx_pool.push_response(MockTxPoolResponse::new(
        &[&tx2, &tx3],
        TransactionFiltered::NotFiltered,
    ));
    mock_tx_pool.push_response(MockTxPoolResponse::new(
        &[&tx4],
        TransactionFiltered::NotFiltered,
    ));
    mock_tx_pool.push_response(MockTxPoolResponse::new(
        &[],
        TransactionFiltered::NotFiltered,
    ));

    // Must not panic.
    let _ = future.await.unwrap().into_result();

    let reports = sink.reports();
    // No leak / no dropped handle: every dispatched batch reported exactly once.
    assert_eq!(
        reports.len(),
        sink.created(),
        "every dispatched batch's feedback handle must fire exactly once (no leak)",
    );
    assert_eq!(sink.created(), 3, "three non-empty batches were dispatched",);
    // At least the fallback-consumed batch reports discarded work (completed=false).
    assert!(
        reports.iter().any(|r| !r.completed),
        "a fallback-discarded batch must report completed=false, got {reports:?}",
    );
}

// ---------------------------------------------------------------------------
// Audit-fix regression tests (coalescing / mint-on-merged-view / blob assert).
// ---------------------------------------------------------------------------

/// Commit `changes` the way the *strict* storage layer does. Every entry of a
/// `ChangesList` is applied, in order, into a single `Fail`-policy transaction
/// that rejects a key appearing in two different entries — the exact
/// same-key conflict detection performed by `RocksDb::commit_changes`' conflict
/// finder (and the scheduler's own blob-path commit). A block whose parallel
/// batches split an `Insert` and a `Remove` of one key across entries is
/// rejected here unless the executor coalesced them first.
fn assert_commits_without_conflict(changes: &StorageChanges) {
    let base = InMemoryStorage::<Column>::default();
    let mut tx =
        StorageTransaction::transaction(&base, ConflictPolicy::Fail, Changes::default());
    match changes {
        StorageChanges::Changes(c) => {
            tx.commit_changes(c.clone())
                .expect("changes must commit cleanly");
        }
        StorageChanges::ChangesList(list) => {
            for (i, c) in list.iter().enumerate() {
                tx.commit_changes(c.clone()).unwrap_or_else(|e| {
                    panic!("conflict committing ChangesList entry {i}: {e}")
                });
            }
        }
    }
}

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

// TEST 1 (integration level) — a block with a coin created in one batch and
// spent in a later batch. Before the coalescing fix the produced changes are a
// ChangesList whose Insert/Remove of the coin land in different entries and the
// strict conflict finder rejects the whole block; after the fix they coalesce
// to a single Changes that commits cleanly.
#[tokio::test]
async fn execute__coin_created_and_spent_in_later_batch_commits_cleanly() {
    let mut rng = rand::rngs::StdRng::seed_from_u64(2322);
    let mut storage = Storage::default();
    storage = add_consensus_parameters(storage, &ConsensusParameters::default());
    let recipient_private_key = SecretKey::random(&mut rng);
    let owner = Input::owner(&recipient_private_key.public_key());

    // tx1 (batch 0) creates a coin; tx2 (batch 1) spends it.
    let tx1 = TransactionBuilder::script(vec![], vec![])
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
                worker_count: std::num::NonZeroUsize::new(2).unwrap(),
                worker_count_policy: crate::config::WorkerCountPolicy::StaticMax,
                metrics: false,
                utxo_validation: true,
            },
        )
        .unwrap();
    let (transactions_source, mock_tx_pool) = MockTransactionsSource::new();

    let future = executor.produce_without_commit_with_source(
        Components {
            header_to_produce: Default::default(),
            transactions_source,
            coinbase_recipient: Default::default(),
            gas_price: 0,
        },
        Instant::now() + Duration::from_millis(300),
    );
    // batch 0: creator; batch 1: spender (forces the cross-batch split).
    mock_tx_pool.push_response(MockTxPoolResponse::new(
        &[&tx1],
        TransactionFiltered::NotFiltered,
    ));
    mock_tx_pool.push_response(MockTxPoolResponse::new(
        &[&tx2],
        TransactionFiltered::NotFiltered,
    ));
    mock_tx_pool.push_response(MockTxPoolResponse::new(
        &[],
        TransactionFiltered::NotFiltered,
    ));

    let (result, changes) = future.await.unwrap().into();

    // both txs made it into the block (+ mint)
    assert_eq!(result.block.transactions().len(), 3);
    // the block's changes are a single, coalesced map ...
    assert!(
        matches!(changes, StorageChanges::Changes(_)),
        "expected a single coalesced Changes, got a ChangesList",
    );
    // ... and commit cleanly through the strict conflict finder.
    assert_commits_without_conflict(&changes);
}

// TEST 3 (mint on the merged view) — a user tx that touches the coinbase
// contract plus the mint tx. The mint updates the coinbase contract's UTXO, and
// so does the user tx; before the fix those two writes land in different
// ChangesList entries (mint ran on a fresh pre-block view and was appended as
// its own entry) and the strict conflict finder rejects the block. After the
// fix the mint executes against the fully-merged view and its changes fold into
// a single Changes that commits cleanly — and the coinbase accounting is
// computed against the same-block contract state, not a stale pre-block one.
#[tokio::test]
async fn execute__user_tx_touching_coinbase_contract_then_mint_commits_cleanly() {
    let mut rng = rand::rngs::StdRng::seed_from_u64(2322);
    let (contract_id, contract_changes) = contract_creation_changes(&mut rng).await;
    let mut storage = Storage::default();
    storage.merge_changes(contract_changes).unwrap();
    storage = add_consensus_parameters(storage, &ConsensusParameters::default());

    // A user tx that calls the coinbase contract (touches its UTXO).
    let tx_call: Transaction = TransactionBuilder::script(vec![], vec![])
        .add_input(Input::contract(
            rng.r#gen(),
            Default::default(),
            Default::default(),
            Default::default(),
            contract_id,
        ))
        .add_stored_coin_input(&mut rng, &mut storage, 1_000_000)
        .add_output(Output::contract(0, Default::default(), Default::default()))
        .finalize_as_transaction();

    let mut executor: Executor<Storage, MockRelayer, MockPreconfirmationSender> =
        Executor::new(
            storage,
            MockRelayer,
            MockPreconfirmationSender,
            Config {
                worker_count: std::num::NonZeroUsize::new(2).unwrap(),
                worker_count_policy: crate::config::WorkerCountPolicy::StaticMax,
                metrics: false,
                utxo_validation: true,
            },
        )
        .unwrap();
    let (transactions_source, mock_tx_pool) = MockTransactionsSource::new();

    let future = executor.produce_without_commit_with_source(
        Components {
            header_to_produce: Default::default(),
            transactions_source,
            // The coinbase recipient IS the contract the user tx touches.
            coinbase_recipient: contract_id,
            gas_price: 0,
        },
        Instant::now() + Duration::from_millis(300),
    );
    mock_tx_pool.push_response(MockTxPoolResponse::new(
        &[&tx_call],
        TransactionFiltered::NotFiltered,
    ));
    mock_tx_pool.push_response(MockTxPoolResponse::new(
        &[],
        TransactionFiltered::NotFiltered,
    ));

    let (result, changes) = future.await.unwrap().into();

    // user tx + mint tx
    assert_eq!(result.block.transactions().len(), 2);
    assert!(
        matches!(changes, StorageChanges::Changes(_)),
        "expected a single coalesced Changes, got a ChangesList",
    );
    assert_commits_without_conflict(&changes);
}

// TEST 4 (blob debug assert) — `add_blob_execution_data` must carry the merged
// blob changes through. The old code asserted `self.changes.is_empty()`
// immediately after assigning the non-empty merged changes into it, panicking
// every debug-build block that contained a blob. This is the unit-level proof
// (a full blob block through the mock harness needs a valid blob tx, which the
// mock source does not build).
#[test]
fn add_blob_execution_data__carries_changes_and_does_not_panic() {
    let mut res = SchedulerExecutionResult::default();

    let mut blob_changes = ExecutionData::new();
    blob_changes.changes = single_op_changes(Column::Coins, coins_key(), insert_op(1));
    blob_changes.used_gas = 42;

    // Would panic here in a debug build before the fix.
    res.add_blob_execution_data(blob_changes, vec![]);

    assert!(
        !res.changes.is_empty(),
        "blob execution data must be carried through, not asserted empty",
    );
    assert_eq!(res.used_gas, 42);
}

// ---------------------------------------------------------------------------
// Package B — sequential-replay oracle (systematic regression net).
//
// For each scenario the parallel producer builds a block (via the existing
// `produce_without_commit_with_source` mock harness, forcing multi-batch splits
// through the mock's response queue), then the SAME committed tx order is
// re-executed by the plain sequential `fuel-core-executor`
// `ExecutionInstance::produce_without_commit` — the PRIMARY reference, run from
// the same starting view and the same `ExecutionOptions`, in a single storage
// transaction. We then assert the two agree on: folded final state (key-for-key
// `Changes`), committed order, mint/coinbase, tx statuses/receipts/gas/fees, and
// skipped-tx sets.
//
// All scenarios PASS on the current (post-audit-fix) code; the value is
// regression protection. A FAILURE here is a NEW BUG (the assertions are
// deliberately strict and must not be weakened to go green).
//
// Concurrency note: the mock `TransactionsSource` does NOT honour the
// scheduler's per-contract exclusion (the real txpool does), so two
// concurrently-dispatched batches touching the SAME contract would violate the
// scheduler's one-writer-per-contract invariant. The hand-written multi-worker
// scenarios therefore keep each contract within a single batch; the
// same-contract serial-chain and the random workloads use a single worker
// (still exercising cross-batch fold/handoff/fallback across varied batch
// boundaries). Cross-batch coin spends legitimately route through the sequential
// fallback regardless of worker count.
// ---------------------------------------------------------------------------

const ORACLE_DEADLINE_MS: u64 = 400;

// The sequential reference executor refuses to run a height-0 (genesis) block,
// and both producers look up the previous block for DA processing, so the oracle
// produces block height 1 with an empty previous block at height 0 (da_height 0
// == the produced header's da_height, so no DA events are processed on either
// side).
fn add_previous_block(storage: &mut Storage) {
    let mut block = Block::default();
    block.header_mut().set_da_height(0u64.into());
    block.header_mut().recalculate_metadata();
    let compressed = block.compress(&ChainId::default());
    let mut tx = storage.0.write_transaction();
    tx.storage_as_mut::<FuelBlocks>()
        .insert(&0u32.into(), &compressed)
        .unwrap();
    tx.commit().unwrap();
}

fn header_at_height_1() -> PartialBlockHeader {
    let mut header = PartialBlockHeader::default();
    header.consensus.height = 1u32.into();
    header
}

fn split_off_mint(
    txs: &[Transaction],
    scenario: &str,
) -> (Transaction, Vec<Transaction>) {
    let (last, rest) = txs
        .split_last()
        .unwrap_or_else(|| panic!("[{scenario}] produced block has no transactions"));
    assert!(
        last.is_mint(),
        "[{scenario}] expected the last block tx to be the mint, got {last:?}",
    );
    (last.clone(), rest.to_vec())
}

type StatusSummary = (Bytes32, bool, u64, u64, Vec<Receipt>);

fn status_summary(s: &TransactionExecutionStatus) -> StatusSummary {
    let success = matches!(s.result, TransactionExecutionResult::Success { .. });
    (
        s.id,
        success,
        *s.result.total_gas(),
        *s.result.total_fee(),
        s.result.receipts().to_vec(),
    )
}

// Compare statuses as a by-id-sorted multiset: every tx's status/receipts/gas/fee
// must match, independent of the order the two executors emit them in (block
// order is asserted separately via the committed-tx-id list).
fn sorted_summaries(v: &[TransactionExecutionStatus]) -> Vec<StatusSummary> {
    let mut out: Vec<StatusSummary> = v.iter().map(status_summary).collect();
    out.sort_by_key(|s| s.0);
    out
}

fn tx_ids(txs: &[Transaction]) -> Vec<Bytes32> {
    txs.iter().map(|tx| tx.id(&ChainId::default())).collect()
}

/// Drop per-column buckets that hold no operations. An empty bucket is a no-op
/// at commit time (it changes no keys), so two `Changes` that differ only in
/// vacuous empty buckets describe the identical database state.
fn strip_empty_columns(mut changes: Changes) -> Changes {
    changes.retain(|_, ops| !ops.is_empty());
    changes
}

/// Run the parallel producer over `batches`, then replay its committed order
/// through the sequential reference executor and assert full agreement. Uses the
/// default (comfortable) block deadline, so batches complete in the scheduler's
/// main loop.
async fn run_replay_oracle(
    storage: Storage,
    coinbase_recipient: ContractId,
    gas_price: u64,
    batches: Vec<Vec<Transaction>>,
    worker_count: usize,
    scenario: &str,
) {
    run_replay_oracle_with_deadline(
        storage,
        coinbase_recipient,
        gas_price,
        batches,
        worker_count,
        scenario,
        Instant::now() + Duration::from_millis(ORACLE_DEADLINE_MS),
    )
    .await
}

/// As [`run_replay_oracle`], but with a caller-chosen `deadline` so a scenario can
/// force batches to complete on the end-of-block drain path
/// (`wait_all_execution_tasks`) by passing an already-elapsed deadline.
#[allow(clippy::too_many_arguments)]
async fn run_replay_oracle_with_deadline(
    storage: Storage,
    coinbase_recipient: ContractId,
    gas_price: u64,
    batches: Vec<Vec<Transaction>>,
    worker_count: usize,
    scenario: &str,
    deadline: Instant,
) {
    run_replay_oracle_with_deadline_and_utxo_validation(
        storage,
        coinbase_recipient,
        gas_price,
        batches,
        worker_count,
        scenario,
        deadline,
        true,
    )
    .await
}

/// As [`run_replay_oracle_with_deadline`], but with a caller-chosen
/// `utxo_validation` mode. `utxo_validation = false` is the supported
/// relaxed/debugging mode where input coins are NOT required to exist in the
/// database; both producers are configured the way the node wires them in that
/// mode (parallel: `Config::utxo_validation`; sequential:
/// `forbid_unauthorized_inputs = utxo_validation`), and must still agree on
/// everything.
#[allow(clippy::too_many_arguments)]
async fn run_replay_oracle_with_deadline_and_utxo_validation(
    mut storage: Storage,
    coinbase_recipient: ContractId,
    gas_price: u64,
    batches: Vec<Vec<Transaction>>,
    worker_count: usize,
    scenario: &str,
    deadline: Instant,
    utxo_validation: bool,
) {
    let fed_tx_count: usize = batches.iter().map(|b| b.len()).sum();
    // Both producers start from the same view, which now includes the previous
    // block so height-1 production is accepted by both.
    add_previous_block(&mut storage);
    let storage_for_seq = storage.clone();

    // ---- parallel producer ----
    let mut executor: Executor<Storage, MockRelayer, MockPreconfirmationSender> =
        Executor::new(
            storage,
            MockRelayer,
            MockPreconfirmationSender,
            Config {
                worker_count: std::num::NonZeroUsize::new(worker_count).unwrap(),
                worker_count_policy: crate::config::WorkerCountPolicy::StaticMax,
                metrics: false,
                utxo_validation,
            },
        )
        .unwrap();
    let (source, pool) = MockTransactionsSource::new();
    for batch in &batches {
        let refs: Vec<&Transaction> = batch.iter().collect();
        pool.push_response(MockTxPoolResponse::new(
            &refs,
            TransactionFiltered::NotFiltered,
        ));
    }
    let (par_result, par_changes) = executor
        .produce_without_commit_with_source(
            Components {
                header_to_produce: header_at_height_1(),
                transactions_source: source,
                coinbase_recipient,
                gas_price,
            },
            deadline,
        )
        .await
        .unwrap()
        .into();

    // The parallel path must emit ONE coalesced `Changes` (audit fix #1).
    let par_changes = match par_changes {
        StorageChanges::Changes(c) => c,
        StorageChanges::ChangesList(_) => panic!(
            "[{scenario}] parallel producer must emit a single coalesced Changes, \
             got a ChangesList (NEW BUG)"
        ),
    };
    let par_block_txs = par_result.block.transactions().to_vec();
    let (par_mint, par_user) = split_off_mint(&par_block_txs, scenario);
    // Upper bound only: the producer never commits MORE than it was fed. The
    // exact committed set is pinned by the `par_user == seq_user` comparison
    // below (which also catches under-inclusion — a tx the producer dropped but
    // the reference keeps). We do NOT assert every fed tx committed, because a
    // runtime-failing tx (a bad predicate tripping the fallback) is legitimately
    // dropped by both executors.
    assert!(
        par_user.len() <= fed_tx_count,
        "[{scenario}] parallel producer committed more txs ({}) than fed ({})",
        par_user.len(),
        fed_tx_count,
    );

    // ---- sequential reference ----
    // Feed the reference the ORIGINAL fed txs (flattened, in fed order), NOT
    // just the parallel block's committed txs, so it INDEPENDENTLY decides which
    // txs to commit vs skip. This is what lets the oracle validate a
    // fallback-triggering scenario: a runtime-failing tx (bad predicate) is
    // dropped by both executors, and the committed sets must still match. Note
    // the parallel producer does NOT surface L2 runtime-skips in
    // `skipped_transactions` (every batch-level skip is consumed by the
    // fallback, which re-executes only the surviving txs and drops the skipped
    // one silently), so we compare the COMMITTED sets rather than the skip
    // metadata. For a skip-free scenario the fed txs equal the committed txs, so
    // this is identical to the old harness.
    let options = ExecutionOptions {
        forbid_unauthorized_inputs: utxo_validation,
        forbid_fake_utxo: false,
        allow_syscall: false,
    };
    let fed_flat: Vec<Transaction> = batches.iter().flatten().cloned().collect();
    let seq_source = OnceTransactionsSource::new(fed_flat);
    let components = Components {
        header_to_produce: header_at_height_1(),
        transactions_source: seq_source,
        coinbase_recipient,
        gas_price,
    };
    let (seq_result, seq_changes) = ExecutionInstance::new(
        MockRelayer,
        storage_for_seq,
        options,
        MemoryInstance::new(),
    )
    .produce_without_commit(
        components,
        false,
        TimeoutOnlyTxWaiter,
        TransparentPreconfirmationSender,
    )
    .await
    .unwrap()
    .into();

    let seq_block_txs = seq_result.block.transactions().to_vec();
    let (seq_mint, seq_user) = split_off_mint(&seq_block_txs, scenario);

    // (1) final state: folded parallel Changes == sequential Changes, key-for-key.
    // Empty per-column buckets are stripped first: they carry no keys (committing
    // one is a no-op, so they are not a state difference), and the sequential
    // reference can leave a vacuous empty bucket behind after processing-then-
    // skipping a runtime-failing tx that the parallel fallback never replays.
    assert_eq!(
        strip_empty_columns(par_changes),
        strip_empty_columns(seq_changes),
        "[{scenario}] final-state mismatch: folded parallel Changes != sequential \
         Changes (NEW BUG)",
    );
    // (2) committed user-tx order.
    assert_eq!(
        tx_ids(&par_user),
        tx_ids(&seq_user),
        "[{scenario}] committed user-tx order mismatch (NEW BUG)",
    );
    // (3) mint tx (encodes the coinbase amount).
    assert_eq!(
        par_mint, seq_mint,
        "[{scenario}] mint tx / coinbase amount mismatch (NEW BUG)",
    );
    // (4) statuses / receipts / gas / fees.
    assert_eq!(
        sorted_summaries(&par_result.tx_status),
        sorted_summaries(&seq_result.tx_status),
        "[{scenario}] tx status/receipts/gas/fee mismatch (NEW BUG)",
    );
    // (5) block used_gas (sum over statuses) — explicit though implied by (4).
    let par_gas: u64 = par_result
        .tx_status
        .iter()
        .map(|s| *s.result.total_gas())
        .sum();
    let seq_gas: u64 = seq_result
        .tx_status
        .iter()
        .map(|s| *s.result.total_gas())
        .sum();
    assert_eq!(par_gas, seq_gas, "[{scenario}] used_gas mismatch (NEW BUG)");
}

// A script tx that takes one contract input/output per contract in `contracts`
// (indices 0..n) plus a funding coin input.
fn contract_call_tx(
    rng: &mut StdRng,
    storage: &mut Storage,
    contracts: &[ContractId],
) -> Transaction {
    let mut builder = TransactionBuilder::script(vec![], vec![]);
    for (i, contract) in contracts.iter().enumerate() {
        builder.add_input(Input::contract(
            rng.r#gen(),
            Default::default(),
            Default::default(),
            Default::default(),
            *contract,
        ));
        builder.add_output(Output::contract(
            i as u16,
            Default::default(),
            Default::default(),
        ));
    }
    builder.add_stored_coin_input(rng, storage, 1000);
    builder.finalize_as_transaction()
}

// A tx that spends a stored coin and creates a fresh coin output owned by
// `new_key`; returns the tx and the spendable UtxoId of its output (gas_price 0,
// so out-amount == in-amount).
fn coin_creating_tx(
    rng: &mut StdRng,
    storage: &mut Storage,
    new_key: &SecretKey,
    amount: u64,
) -> (Transaction, UtxoId) {
    let owner = Input::owner(&new_key.public_key());
    let tx = TransactionBuilder::script(vec![], vec![])
        .add_stored_coin_input(rng, storage, amount)
        .add_output(Output::coin(owner, amount, Default::default()))
        .finalize_as_transaction();
    let utxo = UtxoId::new(tx.id(&ChainId::default()), 0);
    (tx, utxo)
}

// A `Create` transaction that deploys a fresh contract; returns the tx (to feed
// into a batch) and the id of the contract it creates. Deploying the contract
// *inside the block* (rather than pre-seeding storage) is what makes the
// fallback-soundness bug observable: a kept batch's contract creation lives only
// in the accumulated in-block state, so if the fallback replays a later batch
// that calls the contract against the stale pre-block view, the contract does
// not exist and the call is wrongly skipped.
fn create_contract_tx(
    rng: &mut StdRng,
    storage: &mut Storage,
) -> (Transaction, ContractId) {
    let tx = TransactionBuilder::create(
        Default::default(),
        Salt::new(rng.r#gen()),
        Default::default(),
    )
    .add_stored_coin_input(rng, storage, 1000)
    .add_contract_created()
    .finalize_as_transaction();
    let contract_id = tx
        .outputs()
        .first()
        .and_then(|output| output.contract_id())
        .cloned()
        .expect("Create tx must have a ContractCreated output");
    (tx, contract_id)
}

// A tx whose coin-predicate input mis-declares its predicate gas (declares 0 but
// the predicate actually costs gas), which the executor rejects at runtime — the
// tx is SKIPPED, which is exactly what triggers the scheduler's sequential
// fallback. The same tx is skipped identically by the sequential reference, so
// the oracle's skip-set comparison stays balanced. Mirrors the trigger used by
// `execute__trigger_skipped_txs_fallback_mechanism` /
// `feedback__fallback_path_reports_completed_false_and_does_not_leak`.
fn fallback_trigger_tx(rng: &mut StdRng, storage: &mut Storage) -> Transaction {
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

    let mut builder = TransactionBuilder::script(vec![], vec![]);
    builder.add_stored_coin_input(rng, storage, 1000);
    builder.add_input(Input::coin_predicate(
        utxo_id,
        owner,
        amount,
        Default::default(),
        Default::default(),
        Default::default(),
        code_bytes,
        vec![],
    ));
    builder.finalize_as_transaction()
}

async fn setup_contracts(
    rng: &mut StdRng,
    storage: &mut Storage,
    n: usize,
) -> Vec<ContractId> {
    let mut contracts = Vec::with_capacity(n);
    for _ in 0..n {
        let (cid, changes) = contract_creation_changes(rng).await;
        storage.merge_changes(changes).unwrap();
        contracts.push(cid);
    }
    contracts
}

// SCENARIO 1 — multi-contract txs spread across batches (distinct contracts per
// batch, so real 2-worker parallelism is safe): exercises multi-contract
// per-contract Changes handoff + cross-batch merge + fold.
#[tokio::test]
async fn oracle__multi_contract_txs_across_batches() {
    let mut rng = rand::rngs::StdRng::seed_from_u64(2322);
    let mut storage = Storage::default();
    let contracts = setup_contracts(&mut rng, &mut storage, 4).await;
    storage = add_consensus_parameters(storage, &ConsensusParameters::default());

    let b0 = vec![
        contract_call_tx(&mut rng, &mut storage, &[contracts[0], contracts[1]]),
        contract_call_tx(&mut rng, &mut storage, &[contracts[0]]),
    ];
    let b1 = vec![
        contract_call_tx(&mut rng, &mut storage, &[contracts[2], contracts[3]]),
        contract_call_tx(&mut rng, &mut storage, &[contracts[3]]),
    ];

    run_replay_oracle(
        storage,
        Default::default(),
        0,
        vec![b0, b1],
        2,
        "multi_contract_across_batches",
    )
    .await;
}

// SCENARIO 2 — cross-batch coin create-then-spend chain (3 links across 3
// batches). The later batches' spenders cannot see not-yet-committed coins, so
// this routes through the sequential fallback; the oracle proves the fallback's
// result matches a straight sequential run.
#[tokio::test]
async fn oracle__cross_batch_coin_create_then_spend_chain() {
    let mut rng = rand::rngs::StdRng::seed_from_u64(2322);
    let mut storage = Storage::default();
    storage = add_consensus_parameters(storage, &ConsensusParameters::default());

    let k1 = SecretKey::random(&mut rng);
    let (tx1, utxo1) = coin_creating_tx(&mut rng, &mut storage, &k1, 1000);
    // tx2 spends coinA (from k1) and creates coinB (to k2).
    let k2 = SecretKey::random(&mut rng);
    let owner2 = Input::owner(&k2.public_key());
    let tx2 = TransactionBuilder::script(vec![], vec![])
        .add_unsigned_coin_input(k1, utxo1, 1000, Default::default(), Default::default())
        .add_output(Output::coin(owner2, 1000, Default::default()))
        .finalize_as_transaction();
    let utxo2 = UtxoId::new(tx2.id(&ChainId::default()), 0);
    // tx3 spends coinB.
    let owner3 = Input::owner(&SecretKey::random(&mut rng).public_key());
    let tx3 = TransactionBuilder::script(vec![], vec![])
        .add_unsigned_coin_input(k2, utxo2, 1000, Default::default(), Default::default())
        .add_output(Output::coin(owner3, 1000, Default::default()))
        .finalize_as_transaction();

    run_replay_oracle(
        storage,
        Default::default(),
        0,
        vec![vec![tx1], vec![tx2], vec![tx3]],
        2,
        "cross_batch_coin_chain",
    )
    .await;
}

// SCENARIO 2b — `utxo_validation = false` (relaxed/debugging mode): input coins
// are NOT required to exist in the database. The sequential executor fabricates
// missing coins (`get_coin_or_default`), so the parallel producer must accept
// the same "fake"-coin transactions instead of failing the whole block with
// "Coin ... not in the database and not created in the block". Mixes fake-coin
// txs with a stored-coin tx across two batches and asserts full agreement with
// the sequential run in the same mode.
#[tokio::test]
async fn oracle__utxo_validation_off_accepts_fake_coin_inputs() {
    let mut rng = rand::rngs::StdRng::seed_from_u64(2322);
    let mut storage = Storage::default();
    storage = add_consensus_parameters(storage, &ConsensusParameters::default());

    // Coins that exist nowhere: not in the database, not created in the block.
    let fake_tx1 = TransactionBuilder::script(vec![], vec![])
        .add_fake_coin_input(&mut rng, 1000)
        .finalize_as_transaction();
    let fake_tx2 = TransactionBuilder::script(vec![], vec![])
        .add_fake_coin_input(&mut rng, 500)
        .finalize_as_transaction();
    // A regular stored coin still works in relaxed mode.
    let stored_tx = basic_tx(&mut rng, &mut storage);

    run_replay_oracle_with_deadline_and_utxo_validation(
        storage,
        Default::default(),
        0,
        vec![vec![fake_tx1], vec![fake_tx2, stored_tx]],
        2,
        "utxo_validation_off_fake_coins",
        Instant::now() + Duration::from_millis(ORACLE_DEADLINE_MS),
        false,
    )
    .await;
}

// Negative control for SCENARIO 2b — with `utxo_validation = true` (the
// production default) the coin coherency verifier must keep rejecting a coin
// that is neither in the database nor created in the block, exactly as before.
#[tokio::test]
async fn execute__utxo_validation_on_still_rejects_fake_coin_inputs() {
    let mut rng = rand::rngs::StdRng::seed_from_u64(2322);
    let mut storage = Storage::default();
    storage = add_consensus_parameters(storage, &ConsensusParameters::default());

    let fake_tx = TransactionBuilder::script(vec![], vec![])
        .add_fake_coin_input(&mut rng, 1000)
        .finalize_as_transaction();

    let mut executor = Executor::new(
        storage,
        MockRelayer,
        MockPreconfirmationSender,
        Config {
            worker_count: std::num::NonZeroUsize::new(1)
                .expect("The value is not zero; qed"),
            worker_count_policy: crate::config::WorkerCountPolicy::StaticMax,
            metrics: false,
            utxo_validation: true,
        },
    )
    .unwrap();
    let (transactions_source, mock_tx_pool) = MockTransactionsSource::new();
    mock_tx_pool.push_response(
        MockTxPoolResponse::new(&[&fake_tx], TransactionFiltered::NotFiltered)
            .assert_filter(empty_filter()),
    );

    let result = executor
        .produce_without_commit_with_source(
            Components {
                header_to_produce: Default::default(),
                transactions_source,
                coinbase_recipient: Default::default(),
                gas_price: 0,
            },
            Instant::now() + Duration::from_millis(300),
        )
        .await;

    let err = result.expect_err(
        "strict mode must keep rejecting a coin that exists neither in the \
         database nor in the block",
    );
    assert!(
        err.to_string()
            .contains("not in the database and not created in the block"),
        "unexpected error: {err}",
    );
}

// SCENARIO 3 — a user tx that touches the coinbase contract, plus the mint. With
// gas_price > 0 the coinbase is non-zero, so this checks mint-on-the-merged-view
// (audit fix #3): the mint's coinbase-contract write must coalesce with the user
// tx's and the coinbase accounting must match the sequential run.
#[tokio::test]
async fn oracle__user_tx_touching_coinbase_contract_plus_mint() {
    let mut rng = rand::rngs::StdRng::seed_from_u64(2322);
    let mut storage = Storage::default();
    let (contract_id, contract_changes) = contract_creation_changes(&mut rng).await;
    storage.merge_changes(contract_changes).unwrap();
    storage = add_consensus_parameters(storage, &ConsensusParameters::default());

    let mut builder = TransactionBuilder::script(vec![], vec![]);
    builder
        .max_fee_limit(1_000_000) // gas_price > 0 needs a fee budget
        .add_input(Input::contract(
            rng.r#gen(),
            Default::default(),
            Default::default(),
            Default::default(),
            contract_id,
        ))
        .add_stored_coin_input(&mut rng, &mut storage, 1_000_000)
        .add_output(Output::contract(0, Default::default(), Default::default()));
    let tx_call = builder.finalize_as_transaction();

    run_replay_oracle(
        storage,
        contract_id, // coinbase recipient IS the touched contract
        1,           // non-zero gas price => non-zero coinbase
        vec![vec![tx_call]],
        2,
        "coinbase_contract_plus_mint",
    )
    .await;
}

// SCENARIO 4 — a contested single contract as a serial chain: every tx touches
// the same contract, one per batch, single worker (serial). Exercises repeated
// cross-batch handoff of one contract's Changes.
#[tokio::test]
async fn oracle__contested_single_contract_serial_chain() {
    let mut rng = rand::rngs::StdRng::seed_from_u64(2322);
    let mut storage = Storage::default();
    let contracts = setup_contracts(&mut rng, &mut storage, 1).await;
    storage = add_consensus_parameters(storage, &ConsensusParameters::default());
    let c = contracts[0];

    let batches = vec![
        vec![contract_call_tx(&mut rng, &mut storage, &[c])],
        vec![contract_call_tx(&mut rng, &mut storage, &[c])],
        vec![contract_call_tx(&mut rng, &mut storage, &[c])],
    ];

    run_replay_oracle(
        storage,
        Default::default(),
        0,
        batches,
        1,
        "contested_single_contract",
    )
    .await;
}

// SCENARIO 4b — FALLBACK SOUNDNESS (audit finding #5). A kept batch (batch 0)
// creates a contract; a LATER batch (batch 1) both calls that contract AND
// contains a runtime-failing tx (a bad predicate) that trips the sequential
// fallback. The fallback re-executes batch 1's range, which must observe the
// contract created by the kept batch. Before the fix the replay ran against the
// bare pre-block view: the contract did not exist, so the call was wrongly
// skipped and the contract creation was lost entirely — the parallel producer's
// final state and skip set diverged from the sequential reference. After the fix
// the replay is seeded with the as-of-`lower` accumulated state (base + DA +
// kept batches' per-contract changes), so the call succeeds and only the bad
// predicate is skipped, matching the reference exactly.
//
// Single worker so the create batch is fully committed (and thus a KEPT batch
// with id < the fallback's `lower`) before the calling batch runs.
#[tokio::test]
async fn oracle__fallback_replays_range_against_kept_batch_contract_state() {
    let mut rng = rand::rngs::StdRng::seed_from_u64(2322);
    let mut storage = Storage::default();
    storage = add_consensus_parameters(storage, &ConsensusParameters::default());

    // batch 0 (KEPT): create a contract in-block.
    let (create_tx, contract_id) = create_contract_tx(&mut rng, &mut storage);
    // batch 1 (fallback range): a call to the just-created contract, plus a
    // bad-predicate tx that trips the fallback for the whole batch.
    let call_tx = contract_call_tx(&mut rng, &mut storage, &[contract_id]);
    let bad_tx = fallback_trigger_tx(&mut rng, &mut storage);

    run_replay_oracle(
        storage,
        Default::default(),
        0,
        vec![vec![create_tx], vec![call_tx, bad_tx]],
        1,
        "fallback_replays_range_against_kept_batch_contract_state",
    )
    .await;
}

// A contract-calling script heavy enough that its batch is still executing on the
// worker runtime when the (already-elapsed) block deadline breaks the scheduler's
// main loop — so the batch is completed by the end-of-block drain
// (`wait_all_execution_tasks`) rather than the main loop's
// `register_execution_result`. The contract input/output makes the batch write
// per-contract state (`ContractsLatestUtxo`), so a drain path that fails to
// re-insert the batch's `changes_per_contract` into the shared map silently drops
// that write from the final merge (FIX 1). Same busy-loop body as `heavy_tx`.
fn heavy_contract_tx(
    rng: &mut StdRng,
    storage: &mut Storage,
    contract: ContractId,
) -> Transaction {
    let mut ops = vec![op::movi(0x10, 10_000), op::movi(0x11, 0)];
    let loop_start = ops.len();
    ops.push(op::subi(0x10, 0x10, 1));
    for _ in 0..10 {
        ops.push(op::add(0x11, 0x11, 0x10));
    }
    let back = (ops.len() - loop_start) as u16;
    ops.push(op::jnzb(0x10, RegId::ZERO, back));
    ops.push(op::ret(RegId::ONE));
    let script_bytes: Vec<u8> = ops.iter().flat_map(|op| op.to_bytes()).collect();
    let mut builder = TransactionBuilder::script(script_bytes, vec![]);
    builder.add_input(Input::contract(
        rng.r#gen(),
        Default::default(),
        Default::default(),
        Default::default(),
        contract,
    ));
    builder.add_output(Output::contract(0, Default::default(), Default::default()));
    builder.script_gas_limit(50_000_000);
    builder.add_stored_coin_input(rng, storage, 1_000_000);
    builder.finalize_as_transaction()
}

// SCENARIO 6 (FIX 1 repro) — a contract-writing batch that completes on the
// end-of-block DRAIN path. The heavy contract-call tx is still in flight when the
// already-elapsed deadline breaks the main loop, so `wait_all_execution_tasks`
// completes it. Before FIX 1 the drain path inserted the batch's non-contract
// `Changes` into `execution_results` but never re-inserted its
// `changes_per_contract` into the shared `contracts_changes` map — so the
// contract's `ContractsLatestUtxo` write was dropped from the final coalesced
// `Changes` and the parallel producer's state diverged from the sequential
// validator (a consensus split). This oracle asserts key-for-key agreement, so it
// FAILS on the pre-fix scheduler and PASSES after.
#[tokio::test]
async fn oracle__drain_completing_contract_batch_keeps_contract_writes() {
    let mut rng = rand::rngs::StdRng::seed_from_u64(2322);
    let mut storage = Storage::default();
    let contracts = setup_contracts(&mut rng, &mut storage, 1).await;
    storage = add_consensus_parameters(storage, &ConsensusParameters::default());

    let tx = heavy_contract_tx(&mut rng, &mut storage, contracts[0]);

    // Already-elapsed deadline + single worker: the one heavy batch is dispatched,
    // the main loop immediately breaks, and the batch drains.
    run_replay_oracle_with_deadline(
        storage,
        Default::default(),
        0,
        vec![vec![tx]],
        1,
        "drain_completing_contract_batch",
        Instant::now(),
    )
    .await;
}

// FIX 4 — fresh-contract "UTXO input does not exist" on the NEXT block after a
// deploy. Root cause: the txpool validates a contract input via
// `contract_exist` = `ContractsRawCode.contains_key(contract_id)`
// (`adapters/txpool.rs`). A contract deployed in block N by a batch that
// completed on the end-of-block DRAIN path had its per-contract changes — which
// for a `Create` tx include `ContractsRawCode` — DROPPED before the merge (the
// FIX 1 bug), so block N never persisted the code and any caller in block N+1 was
// rejected. It was INTERMITTENT because it only bit when the deploy's batch
// happened to finish during the drain rather than in the main loop (timing
// dependent; interval mode's zero deadline — FIX 3 — made the drain the common
// case). This test forces the deploy onto the drain path (a heavy tx keeps the
// batch in flight past an already-elapsed deadline) and asserts the fresh
// contract's `ContractsRawCode` is present in the committed, folded changes — so
// the next block's `contract_exist` sees it. It fails on the pre-FIX-1 scheduler
// and passes after.
#[tokio::test]
async fn drain_completing_deploy_persists_fresh_contract_code() {
    let mut rng = rand::rngs::StdRng::seed_from_u64(2322);
    let mut storage = Storage::default();
    storage = add_consensus_parameters(storage, &ConsensusParameters::default());
    add_previous_block(&mut storage);

    let heavy = heavy_tx(&mut rng, &mut storage);
    let (create_tx, contract_id) = create_contract_tx(&mut rng, &mut storage);

    let mut executor: Executor<Storage, MockRelayer, MockPreconfirmationSender> =
        Executor::new(
            storage,
            MockRelayer,
            MockPreconfirmationSender,
            Config {
                worker_count: std::num::NonZeroUsize::new(1).unwrap(),
                worker_count_policy: crate::config::WorkerCountPolicy::StaticMax,
                metrics: false,
                utxo_validation: true,
            },
        )
        .unwrap();
    let (source, pool) = MockTransactionsSource::new();
    // One batch containing the heavy tx (keeps the batch in flight) and the
    // deploy. The already-elapsed deadline breaks the main loop while the batch
    // is still running, so it is completed by the drain.
    pool.push_response(MockTxPoolResponse::new(
        &[&heavy, &create_tx],
        TransactionFiltered::NotFiltered,
    ));
    let (_result, par_changes) = executor
        .produce_without_commit_with_source(
            Components {
                header_to_produce: header_at_height_1(),
                transactions_source: source,
                coinbase_recipient: Default::default(),
                gas_price: 0,
            },
            Instant::now(),
        )
        .await
        .unwrap()
        .into();

    let folded = match par_changes {
        StorageChanges::Changes(c) => c,
        StorageChanges::ChangesList(list) => {
            fold_changes_in_canonical_order(list).unwrap()
        }
    };
    let raw_code_col = Column::ContractsRawCode.id();
    let has_code = folded
        .get(&raw_code_col)
        .map(|ops| ops.keys().any(|k| k.as_ref() == contract_id.as_ref()))
        .unwrap_or(false);
    assert!(
        has_code,
        "fresh contract's ContractsRawCode must be committed after a \
         drain-completing deploy (FIX 4 / FIX 1) — otherwise the next block's \
         txpool `contract_exist` rejects callers with 'UTXO input does not exist'",
    );
}

// Smoke test for the time-spend block-summary emit path (metrics ON): produce a
// block through the full scheduler with `metrics: true` so the per-block
// decomposition (`record_block_time_decomposition` + the block-summary log line)
// and every accumulation site actually run. Just asserts the block is produced —
// the value is exercising the metrics-on branch that the other tests
// (`metrics: false`) never touch.
#[tokio::test]
async fn metrics_block_summary_emits_without_panicking() {
    let mut rng = rand::rngs::StdRng::seed_from_u64(2322);
    let mut storage = Storage::default();
    let contracts = setup_contracts(&mut rng, &mut storage, 2).await;
    storage = add_consensus_parameters(storage, &ConsensusParameters::default());
    add_previous_block(&mut storage);

    let b0 = vec![
        contract_call_tx(&mut rng, &mut storage, &[contracts[0]]),
        basic_tx(&mut rng, &mut storage),
    ];
    let b1 = vec![contract_call_tx(&mut rng, &mut storage, &[contracts[1]])];

    let mut executor: Executor<Storage, MockRelayer, MockPreconfirmationSender> =
        Executor::new(
            storage,
            MockRelayer,
            MockPreconfirmationSender,
            Config {
                worker_count: std::num::NonZeroUsize::new(2).unwrap(),
                worker_count_policy: crate::config::WorkerCountPolicy::StaticMax,
                metrics: true,
                utxo_validation: true,
            },
        )
        .unwrap();
    let (source, pool) = MockTransactionsSource::new();
    for batch in [b0, b1] {
        let refs: Vec<&Transaction> = batch.iter().collect();
        pool.push_response(MockTxPoolResponse::new(
            &refs,
            TransactionFiltered::NotFiltered,
        ));
    }
    let (result, _changes) = executor
        .produce_without_commit_with_source(
            Components {
                header_to_produce: header_at_height_1(),
                transactions_source: source,
                coinbase_recipient: Default::default(),
                gas_price: 0,
            },
            Instant::now() + Duration::from_millis(ORACLE_DEADLINE_MS),
        )
        .await
        .unwrap()
        .into();
    // 3 user txs + mint.
    assert_eq!(result.block.transactions().len(), 4);
}

// SCENARIO 5 — seeded pseudo-random workloads mixing all of the above patterns,
// so the oracle explores batch boundaries / fold / fallback combinations the
// hand-written cases don't. Single worker (see the concurrency note above);
// deterministic per seed. Fast enough to run in the normal suite (measured well
// under the 30s budget), so NOT #[ignore]d.
fn build_random_workload(
    seed: u64,
    storage: &mut Storage,
    contracts: &[ContractId],
    n: usize,
) -> Vec<Vec<Transaction>> {
    let mut rng = rand::rngs::StdRng::seed_from_u64(seed ^ 0xA5A5_A5A5);
    let mut batches: Vec<Vec<Transaction>> = vec![];
    let mut current: Vec<Transaction> = vec![];
    // Coins created earlier that a later tx may spend (start/extend a chain).
    let mut spendable: Vec<(SecretKey, UtxoId, u64)> = vec![];

    for _ in 0..n {
        let pick = rng.gen_range(0u8..100);
        let tx = if pick < 8 {
            // runtime-failing tx (bad predicate) — skipped at execution, which
            // trips the sequential fallback. Mixing these into the random
            // workload means the net permanently exercises the fallback replay
            // (and its as-of-`lower` seeding) across varied batch boundaries and
            // alongside contract/coin state, not just in the hand-written case.
            fallback_trigger_tx(&mut rng, storage)
        } else if pick < 28 {
            // plain transfer
            basic_tx(&mut rng, storage)
        } else if pick < 50 {
            // single-contract call
            let c = contracts[rng.gen_range(0..contracts.len())];
            contract_call_tx(&mut rng, storage, &[c])
        } else if pick < 70 {
            // multi-contract call (2..=3 distinct contracts)
            let k = rng.gen_range(2..=std::cmp::min(3, contracts.len()));
            let mut idxs: Vec<usize> = vec![];
            while idxs.len() < k {
                let i = rng.gen_range(0..contracts.len());
                if !idxs.contains(&i) {
                    idxs.push(i);
                }
            }
            let picked: Vec<ContractId> = idxs.iter().map(|&i| contracts[i]).collect();
            contract_call_tx(&mut rng, storage, &picked)
        } else if pick < 85 && !spendable.is_empty() {
            // spend an earlier coin, create a new one (extend a chain)
            let idx = rng.gen_range(0..spendable.len());
            let (key, utxo, amount) = spendable.remove(idx);
            let new_key = SecretKey::random(&mut rng);
            let new_owner = Input::owner(&new_key.public_key());
            let tx = TransactionBuilder::script(vec![], vec![])
                .add_unsigned_coin_input(
                    key,
                    utxo,
                    amount,
                    Default::default(),
                    Default::default(),
                )
                .add_output(Output::coin(new_owner, amount, Default::default()))
                .finalize_as_transaction();
            let new_utxo = UtxoId::new(tx.id(&ChainId::default()), 0);
            spendable.push((new_key, new_utxo, amount));
            tx
        } else {
            // create a fresh coin (potential chain start)
            let new_key = SecretKey::random(&mut rng);
            let (tx, utxo) = coin_creating_tx(&mut rng, storage, &new_key, 1000);
            spendable.push((new_key, utxo, 1000));
            tx
        };
        current.push(tx);
        if rng.gen_bool(0.4) && !current.is_empty() {
            batches.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        batches.push(current);
    }
    batches
}

async fn run_random_seed(seed: u64) {
    let mut setup_rng = rand::rngs::StdRng::seed_from_u64(seed);
    let mut storage = Storage::default();
    let contracts = setup_contracts(&mut setup_rng, &mut storage, 4).await;
    storage = add_consensus_parameters(storage, &ConsensusParameters::default());
    let batches = build_random_workload(seed, &mut storage, &contracts, 30);
    run_replay_oracle(
        storage,
        Default::default(),
        0,
        batches,
        1,
        &format!("random_seed_{seed}"),
    )
    .await;
}

#[tokio::test]
async fn oracle__random_workload_seed_1() {
    run_random_seed(1).await;
}

#[tokio::test]
async fn oracle__random_workload_seed_2() {
    run_random_seed(7).await;
}

#[tokio::test]
async fn oracle__random_workload_seed_3() {
    run_random_seed(42).await;
}
