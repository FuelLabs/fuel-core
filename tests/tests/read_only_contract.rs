//! Integration tests for read-only contract inputs driven through a running
//! node via the client API.
//!
//! A contract input WITHOUT a matching contract output is a read-only contract
//! access: the contract's code runs and its state can be read, but any attempt
//! to mutate its state or balances panics with `ContractIsReadOnly`. This is
//! supported automatically by the node's fuel-vm version — there is no
//! consensus parameter or chain-config flag to enable.

use fuel_core::service::{
    Config,
    FuelService,
};
use fuel_core_client::client::{
    FuelClient,
    types::TransactionStatus,
};
use fuel_core_types::{
    fuel_asm::{
        GTFArgs,
        PanicReason,
        RegId,
        op,
    },
    fuel_tx::{
        Bytes32,
        ContractId,
        CreateMetadata,
        Finalizable,
        Input,
        Receipt,
        StorageSlot,
        Transaction,
        TransactionBuilder,
    },
    fuel_vm::{
        Salt,
        SecretKey,
    },
};
use rand::{
    Rng,
    SeedableRng,
};
use test_helpers::counter_contract;

/// Value pre-stored in the reader contract's slot zero.
const STORED_VALUE: u64 = 7;

async fn start_node() -> (FuelService, FuelClient) {
    let mut config = Config::local_node();
    config.debug = true;
    let srv = FuelService::new_node(config).await.unwrap();
    let client = FuelClient::from(srv.bound_address);
    (srv, client)
}

/// Deploys a contract that only READS storage slot zero and returns its value.
/// Slot zero is pre-populated with `STORED_VALUE`.
async fn deploy_reader_contract(
    client: &FuelClient,
    rng: &mut rand::rngs::StdRng,
) -> ContractId {
    let base_asset_id = *client
        .chain_info()
        .await
        .expect("failed to get chain info")
        .consensus_parameters
        .base_asset_id();

    let code: Vec<u8> = [
        // Make a zeroed 32-byte key on the heap.
        op::movi(0x12, 32),
        op::aloc(0x12),
        // Read the word at slot key zero (no write => read-only friendly).
        op::srw(0x10, 0x11, RegId::HP, 0),
        // Return the read value.
        op::ret(0x10),
    ]
    .into_iter()
    .collect();

    // Pre-populate slot zero so the read returns a known value.
    let mut value = Bytes32::zeroed();
    value.as_mut()[..8].copy_from_slice(&STORED_VALUE.to_be_bytes());

    let salt: Salt = rng.r#gen();
    let tx = TransactionBuilder::create(
        code.into(),
        salt,
        vec![StorageSlot::new(Bytes32::zeroed(), value)],
    )
    .add_unsigned_coin_input(
        SecretKey::random(rng),
        rng.r#gen(),
        u32::MAX as u64,
        base_asset_id,
        Default::default(),
    )
    .add_contract_created()
    .finalize();

    let contract_id = CreateMetadata::compute(&tx).unwrap().contract_id;

    let tx: Transaction = tx.into();
    let status = client.submit_and_await_commit(&tx).await.unwrap();
    assert!(
        matches!(status, TransactionStatus::Success { .. }),
        "contract deploy failed: {status:?}"
    );

    contract_id
}

/// Builds a script that `CALL`s `contract_id` with a contract INPUT but NO
/// matching contract OUTPUT — i.e. a read-only contract access.
fn read_only_call_tx(
    rng: &mut rand::rngs::StdRng,
    contract_id: ContractId,
) -> Transaction {
    let script = [
        op::gtf_args(0x10, RegId::ZERO, GTFArgs::ScriptData),
        op::call(0x10, RegId::ZERO, RegId::ZERO, RegId::CGAS),
        op::ret(RegId::RET),
    ];

    let mut script_data = contract_id.to_vec();
    script_data.extend(0u64.to_be_bytes());
    script_data.extend(0u64.to_be_bytes());

    TransactionBuilder::script(script.into_iter().collect(), script_data)
        .script_gas_limit(1_000_000)
        .add_unsigned_coin_input(
            SecretKey::random(rng),
            rng.r#gen(),
            u32::MAX as u64,
            Default::default(),
            Default::default(),
        )
        .add_input(Input::contract(
            rng.r#gen(),
            rng.r#gen(),
            rng.r#gen(),
            Default::default(),
            contract_id,
        ))
        // NB: intentionally NO `.add_output(Output::contract(..))` — this is what
        // makes the contract input read-only.
        .finalize_as_transaction()
}

#[tokio::test]
async fn read_only_contract_input__can_read_contract_state() {
    let mut rng = rand::rngs::StdRng::seed_from_u64(0xBAADF00D);
    let (_srv, client) = start_node().await;

    // Given: a deployed contract with state in slot zero.
    let contract_id = deploy_reader_contract(&client, &mut rng).await;

    // When: it is called with a contract input but NO contract output.
    let tx = read_only_call_tx(&mut rng, contract_id);
    let status = client.submit_and_await_commit(&tx).await.unwrap();

    // Then: the read-only call succeeds and returns the stored value.
    let TransactionStatus::Success { receipts, .. } = status else {
        panic!("expected read-only call to succeed, got {status:?}");
    };
    assert!(
        !receipts.iter().any(|r| matches!(r, Receipt::Panic { .. })),
        "read-only read must not panic: {receipts:?}"
    );
    let Receipt::Return { val, .. } = receipts[receipts.len() - 2] else {
        panic!("expected a Return receipt, got {receipts:?}");
    };
    assert_eq!(val, STORED_VALUE, "read-only call returned the wrong value");
}

#[tokio::test]
async fn read_only_contract_input__can_be_read_repeatedly() {
    let mut rng = rand::rngs::StdRng::seed_from_u64(0xBAADF00D);
    let (_srv, client) = start_node().await;

    // Given: a deployed contract accessed read-only.
    let contract_id = deploy_reader_contract(&client, &mut rng).await;

    // When / Then: the same contract can be read read-only many times, always
    // returning the same value (nothing is ever mutated).
    for _ in 0..3 {
        let tx = read_only_call_tx(&mut rng, contract_id);
        let status = client.submit_and_await_commit(&tx).await.unwrap();
        let TransactionStatus::Success { receipts, .. } = status else {
            panic!("expected read-only call to succeed, got {status:?}");
        };
        let Receipt::Return { val, .. } = receipts[receipts.len() - 2] else {
            panic!("expected a Return receipt, got {receipts:?}");
        };
        assert_eq!(val, STORED_VALUE);
    }
}

#[tokio::test]
async fn read_only_contract_input__rejects_state_write() {
    let mut rng = rand::rngs::StdRng::seed_from_u64(0xBAADF00D);
    let (_srv, client) = start_node().await;

    // Given: a deployed contract whose call WRITES to its own storage (the
    // counter contract increments slot zero via `SWW`).
    let (_, contract_id) = counter_contract::deploy(&client, &mut rng).await;

    // When: it is called with a contract input but NO contract output.
    let tx = read_only_call_tx(&mut rng, contract_id);
    let status = client.submit_and_await_commit(&tx).await.unwrap();

    // Then: the write is rejected with `ContractIsReadOnly`, so the transaction
    // fails instead of mutating the read-only contract.
    let TransactionStatus::Failure { receipts, .. } = status else {
        panic!("expected the write to a read-only contract to fail, got {status:?}");
    };
    assert!(
        receipts.iter().any(|r| matches!(
            r,
            Receipt::Panic { reason, .. }
                if *reason.reason() == PanicReason::ContractIsReadOnly
        )),
        "expected a ContractIsReadOnly panic receipt, got {receipts:?}"
    );
}

#[tokio::test]
async fn contract_input_with_output_still_allows_state_write() {
    let mut rng = rand::rngs::StdRng::seed_from_u64(0xBAADF00D);
    let (_srv, client) = start_node().await;

    // Given: the same writing (counter) contract.
    let (_, contract_id) = counter_contract::deploy(&client, &mut rng).await;

    // When: it is called the normal read-write way (contract input WITH the
    // matching contract output).
    let (_, value) = counter_contract::increment(&client, &mut rng, contract_id).await;

    // Then: the write succeeds — read-only enforcement does not affect the
    // usual read-write path.
    assert_eq!(value, 1, "read-write contract call should increment state");
}
