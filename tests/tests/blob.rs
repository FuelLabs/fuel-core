#![allow(non_snake_case)]

use fuel_core::{
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
use fuel_core_types::{
    fuel_asm::{
        op,
        GTFArgs,
        RegId,
    },
    fuel_tx::{
        BlobBody,
        BlobId,
        BlobIdExt,
        TransactionBuilder,
    },
    fuel_types::canonical::Serialize,
    fuel_vm::checked_transaction::IntoChecked,
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
        let blob_id = BlobId::compute(&blob_data);
        let tx = TransactionBuilder::blob(BlobBody {
            id: blob_id,
            witness_index: 0,
        })
        .add_witness(blob_data.into())
        .add_random_fee_input()
        .finalize_as_transaction()
        .into_checked(Default::default(), &Default::default())
        .expect("Cannot check transaction");

        let status = self
            .client
            .submit_and_await_commit(tx.transaction())
            .await?;
        Ok((status, blob_id))
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
    .add_random_fee_input()
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
    .add_random_fee_input()
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
