#![allow(non_snake_case)]

use fuel_core::{
    chain_config::StateConfig,
    database::{
        Database,
        database_description::on_chain::OnChain,
    },
    service::{
        Config,
        FuelService,
    },
};
use fuel_core_client::client::{
    FuelClient,
    types::TransactionStatus,
};
use fuel_core_types::{
    fuel_asm::{
        GTFArgs,
        Instruction,
        RegId,
        op,
    },
    fuel_tx::{
        BlobBody,
        BlobId,
        BlobIdExt,
        Finalizable,
        Input,
        TransactionBuilder,
    },
    fuel_types::canonical::Serialize,
    fuel_vm::constraints::reg_key::{
        IS,
        SSP,
        ZERO,
    },
};
use test_helpers::{
    assemble_tx::{
        AssembleAndRunTx,
        SigningAccount,
    },
    config_with_fee,
};
use tokio::io;

struct TestContext {
    _node: FuelService,
    client: FuelClient,
}

impl TestContext {
    async fn new() -> Self {
        let mut config = config_with_fee();
        config.debug = true;

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
        &self,
        blob_data: Vec<u8>,
    ) -> io::Result<(TransactionStatus, BlobId)> {
        self.new_blob_with_input(blob_data, None).await
    }

    async fn new_blob_with_input(
        &self,
        blob_data: Vec<u8>,
        account: Option<SigningAccount>,
    ) -> io::Result<(TransactionStatus, BlobId)> {
        let blob_id = BlobId::compute(&blob_data);
        let mut builder = TransactionBuilder::blob(BlobBody {
            id: blob_id,
            witness_index: 0,
        });
        builder.add_witness(blob_data.into());

        let tx = builder.finalize();

        let status = self
            .client
            .assemble_and_run_tx(&tx.into(), unwrap_account(account))
            .await?;

        Ok((status, blob_id))
    }

    async fn run_script(
        &self,
        script: Vec<Instruction>,
        script_data: Vec<u8>,
        account: Option<SigningAccount>,
    ) -> io::Result<TransactionStatus> {
        self.client
            .run_script(script, script_data, unwrap_account(account))
            .await
    }
}

fn unwrap_account(account: Option<SigningAccount>) -> SigningAccount {
    match account {
        Some(account) => account,
        None => test_helpers::default_signing_wallet(),
    }
}

#[tokio::test]
async fn blob__upload_works() {
    // Given
    let ctx = TestContext::new().await;

    // When
    let (status, blob_id) = ctx
        .new_blob([op::ret(RegId::ONE)].into_iter().collect())
        .await
        .unwrap();
    assert!(matches!(status, TransactionStatus::Success { .. }));

    // Then
    let script = vec![
        op::gtf_args(0x11, RegId::ZERO, GTFArgs::ScriptData),
        op::bsiz(0x20, 0x11), // This panics if blob doesn't exist
        op::ret(RegId::ONE),
    ];
    let script_data = blob_id.to_bytes();
    let tx_status = ctx.run_script(script, script_data, None).await.unwrap();
    assert!(matches!(tx_status, TransactionStatus::Success { .. }));
}

#[tokio::test]
async fn blob__cannot_post_already_existing_blob_in_tx_pool() {
    // Given
    let mut config = Config::local_node();
    config.utxo_validation = false;
    config.gas_price_config.min_exec_gas_price = 1000;
    let params = config
        .snapshot_reader
        .chain_config()
        .consensus_parameters
        .clone();

    let ctx = TestContext::new_with_config(config).await;
    let payload: Vec<u8> = [op::ret(RegId::ONE)].into_iter().collect();
    let (status, _blob_id) = ctx.new_blob(payload.clone()).await.unwrap();
    assert!(matches!(status, TransactionStatus::Success { .. }));

    // When
    // We want to submit blob directly to TxPool, because we test that TxPool
    // will reject the blob if it's already was deployed.
    let blob_id = BlobId::compute(&payload);
    let mut builder = TransactionBuilder::blob(BlobBody {
        id: blob_id,
        witness_index: 0,
    });
    builder.with_params(params);
    builder.max_fee_limit(1_000_000_000);
    builder.add_witness(payload.into());
    builder.add_fee_input();
    let blob = builder.finalize();
    let result = ctx.client.submit_and_await_commit(&blob.into()).await;

    // Then
    let err = result.expect_err("Should fail because of the same blob id");
    assert!(
        err.to_string().contains("BlobId is already taken"),
        "{}",
        err
    );
}

#[tokio::test]
async fn blob__accessing_nonexistent_blob_panics_vm() {
    // Given
    let ctx = TestContext::new().await;
    let blob_id = BlobId::new([0; 32]); // Nonexistent

    // When
    let script = vec![
        op::gtf_args(0x11, RegId::ZERO, GTFArgs::ScriptData),
        op::bsiz(0x20, 0x11), // This panics if blob doesn't exist
        op::ret(RegId::ONE),
    ];
    let script_data = blob_id.to_bytes();
    let tx_status = ctx.run_script(script, script_data, None).await.unwrap();

    // Then
    assert!(matches!(tx_status, TransactionStatus::Failure { .. }));
}

#[tokio::test]
async fn blob__can_be_queried_if_uploaded() {
    // Given
    let ctx = TestContext::new().await;
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
    let ctx = TestContext::new().await;
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
    let predicate_with_blob = vec![
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

    let blob_owner = Input::predicate_owner(blob_predicate.clone());
    let predicate_with_blob_owner = Input::predicate_owner(predicate_with_blob.clone());

    let mut state = StateConfig::local_testnet();

    state.coins[0].owner = blob_owner.into();
    state.coins[1].owner = predicate_with_blob_owner.into();

    let blob_predicate_account = SigningAccount::Predicate {
        predicate: blob_predicate,
        predicate_data: vec![],
    };

    let mut config = Config::local_node_with_state_config(state);
    config.debug = true;
    config.utxo_validation = true;
    config.gas_price_config.min_exec_gas_price = 1000;

    let ctx = TestContext::new_with_config(config).await;

    let bytecode: Vec<u8> = [op::ret(RegId::ONE)].into_iter().collect();
    let (status, blob_id) = ctx
        .new_blob_with_input(bytecode.clone(), Some(blob_predicate_account))
        .await
        .unwrap();
    assert!(matches!(status, TransactionStatus::Success { .. }));

    // Given
    let predicate_with_blob_account = SigningAccount::Predicate {
        predicate: predicate_with_blob,
        predicate_data: blob_id.to_bytes(),
    };

    // When
    let result = ctx
        .run_script(vec![], vec![], Some(predicate_with_blob_account))
        .await;

    // Then
    let status = result.expect("Transaction failed");
    assert!(matches!(status, TransactionStatus::Success { .. }));
}
