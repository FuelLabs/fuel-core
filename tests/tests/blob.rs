#![allow(non_snake_case)]

use fuel_core::{
    chain_config::StateConfig,
    database::{
        database_description::on_chain::OnChain,
        Database,
    },
    service::Config,
};
use fuel_core_bin::FuelService;
use fuel_core_client::client::{
    types::TransactionStatus,
    FuelClient,
};
use fuel_core_storage::transactional::AtomicView;
use fuel_core_types::{
    fuel_asm::{
        op,
        GTFArgs,
        Instruction,
        RegId,
    },
    fuel_tx::{
        BlobBody,
        BlobId,
        BlobIdExt,
        Finalizable,
        Input,
        Transaction,
        TransactionBuilder,
    },
    fuel_types::canonical::Serialize,
    fuel_vm::{
        checked_transaction::IntoChecked,
        constraints::reg_key::{
            IS,
            SSP,
            ZERO,
        },
        interpreter::{
            ExecutableTransaction,
            MemoryInstance,
        },
    },
};
use tokio::io;

struct TestContext {
    _node: FuelService,
    client: FuelClient,
}
impl TestContext {
    async fn new() -> Self {
        let config = Config {
            debug: true,
            utxo_validation: false,
            ..Config::local_node()
        };

        Self::new_with_config(config).await
    }

    async fn new_with_config(config: Config) -> Self {
        let node = FuelService::from_database(Database::<OnChain>::in_memory(), config)
            .await
            .unwrap();
        let client = FuelClient::from(node.bound_address);

        Self {
            _node: node,
            client,
        }
    }

    async fn new_blob(
        &mut self,
        blob_data: Vec<u8>,
    ) -> io::Result<(TransactionStatus, BlobId)> {
        self.new_blob_with_input(blob_data, None).await
    }

    async fn new_blob_with_input(
        &mut self,
        blob_data: Vec<u8>,
        input: Option<Input>,
    ) -> io::Result<(TransactionStatus, BlobId)> {
        let blob_id = BlobId::compute(&blob_data);
        let mut builder = TransactionBuilder::blob(BlobBody {
            id: blob_id,
            witness_index: 0,
        });
        builder.add_witness(blob_data.into());

        if let Some(input) = input {
            builder.add_input(input);
        } else {
            builder.add_fee_input();
        }

        let tx = builder.finalize();
        let status = self.submit(tx).await?;

        Ok((status, blob_id))
    }

    async fn submit<Tx>(&mut self, mut tx: Tx) -> io::Result<TransactionStatus>
    where
        Tx: ExecutableTransaction,
    {
        let consensus_parameters =
            self.client.chain_info().await.unwrap().consensus_parameters;

        let database = self._node.shared.database.on_chain().latest_view().unwrap();
        tx.estimate_predicates(
            &consensus_parameters.clone().into(),
            MemoryInstance::new(),
            &database,
        )
        .unwrap();

        let tx: Transaction = tx.into();
        let tx = tx
            .into_checked_basic(Default::default(), &consensus_parameters)
            .expect("Cannot check transaction");

        let status = self
            .client
            .submit_and_await_commit(tx.transaction())
            .await?;
        Ok(status)
    }
}

#[tokio::test]
async fn blob__upload_works() {
    // Given
    let mut ctx = TestContext::new().await;

    // When
    let (status, blob_id) = ctx
        .new_blob([op::ret(RegId::ONE)].into_iter().collect())
        .await
        .unwrap();
    assert!(matches!(status, TransactionStatus::Success { .. }));

    // Then
    let script_tx = TransactionBuilder::script(
        vec![
            op::gtf_args(0x11, RegId::ZERO, GTFArgs::ScriptData),
            op::bsiz(0x20, 0x11), // This panics if blob doesn't exist
            op::ret(RegId::ONE),
        ]
        .into_iter()
        .collect(),
        blob_id.to_bytes(),
    )
    .script_gas_limit(1000000)
    .add_fee_input()
    .finalize_as_transaction();
    let tx_status = ctx
        .client
        .submit_and_await_commit(&script_tx)
        .await
        .unwrap();
    assert!(matches!(tx_status, TransactionStatus::Success { .. }));
}

#[tokio::test]
async fn blob__cannot_post_already_existing_blob() {
    // Given
    let mut ctx = TestContext::new().await;
    let payload: Vec<u8> = [op::ret(RegId::ONE)].into_iter().collect();
    let (status, _blob_id) = ctx.new_blob(payload.clone()).await.unwrap();
    assert!(matches!(status, TransactionStatus::Success { .. }));

    // When
    let result = ctx.new_blob(payload).await;

    // Then
    let err = result.expect_err("Should fail because of the same blob id");
    assert!(err.to_string().contains("BlobId is already taken"));
}

#[tokio::test]
async fn blob__accessing_nonexitent_blob_panics_vm() {
    // Given
    let ctx = TestContext::new().await;
    let blob_id = BlobId::new([0; 32]); // Nonexistent

    // When
    let script_tx = TransactionBuilder::script(
        vec![
            op::gtf_args(0x11, RegId::ZERO, GTFArgs::ScriptData),
            op::bsiz(0x20, 0x11), // This panics if blob doesn't exist
            op::ret(RegId::ONE),
        ]
        .into_iter()
        .collect(),
        blob_id.to_bytes(),
    )
    .script_gas_limit(1000000)
    .add_fee_input()
    .finalize_as_transaction();
    let tx_status = ctx
        .client
        .submit_and_await_commit(&script_tx)
        .await
        .unwrap();

    // Then
    assert!(matches!(tx_status, TransactionStatus::Failure { .. }));
}

#[tokio::test]
async fn blob__can_be_queried_if_uploaded() {
    // Given
    let mut ctx = TestContext::new().await;
    let bytecode: Vec<u8> = [op::ret(RegId::ONE)].into_iter().collect();
    let (status, blob_id) = ctx.new_blob(bytecode.clone()).await.unwrap();
    assert!(matches!(status, TransactionStatus::Success { .. }));

    // When
    let queried_blob = ctx
        .client
        .blob(blob_id)
        .await
        .expect("blob query failed")
        .expect("no block returned");

    // Then
    assert_eq!(queried_blob.id, blob_id);
    assert_eq!(queried_blob.bytecode, bytecode);
}

#[tokio::test]
async fn blob__exists_if_uploaded() {
    // Given
    let mut ctx = TestContext::new().await;
    let bytecode: Vec<u8> = [op::ret(RegId::ONE)].into_iter().collect();
    let (status, blob_id) = ctx.new_blob(bytecode.clone()).await.unwrap();
    assert!(matches!(status, TransactionStatus::Success { .. }));

    // When
    let blob_exists = ctx
        .client
        .blob_exists(blob_id)
        .await
        .expect("blob query failed");

    // Then
    assert!(blob_exists);
}

#[tokio::test]
async fn blob__ask_whether_a_nonexisting_blob_exists() {
    // Given
    let ctx = TestContext::new().await;

    // When
    let blob_exists = ctx
        .client
        .blob_exists(Default::default())
        .await
        .expect("blob query failed");

    // Then
    assert!(!blob_exists);
}

#[tokio::test]
async fn predicate_can_load_blob() {
    let blob_predicate = vec![op::ret(RegId::ONE)].into_iter().collect::<Vec<u8>>();

    // Use `LDC` with mode `1` to load the blob into the predicate.
    let predicate = vec![
        // Take the pointer to the predicate data section
        // where the blob ID is stored
        op::gtf(0x10, ZERO, GTFArgs::InputCoinPredicateData as u16),
        // Store the size of the blob
        op::bsiz(0x11, 0x10),
        // Store start of the blob code
        op::move_(0x12, SSP),
        // Subtract the start of the code from the end of the code
        op::sub(0x12, 0x12, IS),
        // Divide the code by the instruction size to get the number of
        // instructions
        op::divi(0x12, 0x12, Instruction::SIZE as u16),
        // Load the blob by `0x10` ID with the `0x11` size
        op::ldc(0x10, ZERO, 0x11, 1),
        // Jump to a new code location
        op::jmp(0x12),
    ]
    .into_iter()
    .collect::<Vec<u8>>();

    let owner = Input::predicate_owner(predicate.clone());
    let blob_owner = Input::predicate_owner(blob_predicate.clone());

    let mut state = StateConfig::local_testnet();

    state.coins[0].owner = blob_owner;
    let blob_coin = state.coins[0].clone();
    let blob_input = Input::coin_predicate(
        blob_coin.utxo_id(),
        blob_owner,
        blob_coin.amount,
        blob_coin.asset_id,
        Default::default(),
        0,
        blob_predicate,
        vec![],
    );

    state.coins[1].owner = owner;
    let predicate_coin = state.coins[1].clone();

    let mut config = Config::local_node_with_state_config(state);
    config.debug = true;
    config.utxo_validation = true;

    let mut ctx = TestContext::new_with_config(config).await;

    let bytecode: Vec<u8> = [op::ret(RegId::ONE)].into_iter().collect();
    let (status, blob_id) = ctx
        .new_blob_with_input(bytecode.clone(), Some(blob_input))
        .await
        .unwrap();
    assert!(matches!(status, TransactionStatus::Success { .. }));

    // Given
    let predicate_data = blob_id.to_bytes();
    let predicate_input = Input::coin_predicate(
        predicate_coin.utxo_id(),
        owner,
        predicate_coin.amount,
        predicate_coin.asset_id,
        Default::default(),
        0,
        predicate,
        predicate_data,
    );

    // When
    let mut builder = TransactionBuilder::script(vec![], vec![]);
    builder.add_input(predicate_input);
    let tx_with_blobed_predicate = builder.finalize();
    let result = ctx.submit(tx_with_blobed_predicate).await;

    // Then
    let status = result.expect("Transaction failed");
    assert!(matches!(status, TransactionStatus::Success { .. }));
}
