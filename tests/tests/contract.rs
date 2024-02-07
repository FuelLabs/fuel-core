use crate::helpers::{
    TestContext,
    TestSetupBuilder,
};
use fuel_core::service::{
    Config,
    FuelService,
};
use fuel_core_client::client::{
    pagination::{
        PageDirection,
        PaginationRequest,
    },
    types::TransactionStatus,
    FuelClient,
};
use fuel_core_types::{
    fuel_asm::*,
    fuel_tx::*,
    fuel_types::canonical::Serialize,
    fuel_vm::*,
};
use rstest::rstest;

const SEED: u64 = 2322;

#[rstest]
#[tokio::test]
async fn test_contract_balance(
    #[values(AssetId::new([1u8; 32]), AssetId::new([0u8; 32]), AssetId::new([16u8; 32]))]
    asset: AssetId,
    #[values(100, 0, 18446744073709551615)] test_balance: u64,
) {
    let mut test_builder = TestSetupBuilder::new(SEED);
    let (_, contract_id) = test_builder.setup_contract(
        vec![],
        Some(vec![(asset, test_balance)]),
        None,
        None,
    );

    // spin up node
    let TestContext {
        client,
        srv: _dont_drop,
        ..
    } = test_builder.finalize().await;

    let balance = client
        .contract_balance(&contract_id, Some(&asset))
        .await
        .unwrap();

    assert_eq!(balance, test_balance);
}

#[rstest]
#[tokio::test]
async fn test_5_contract_balances(
    #[values(PageDirection::Forward, PageDirection::Backward)] direction: PageDirection,
) {
    let mut test_builder = TestSetupBuilder::new(SEED);
    let (_, contract_id) = test_builder.setup_contract(
        vec![],
        Some(vec![
            (AssetId::new([1u8; 32]), 1000),
            (AssetId::new([2u8; 32]), 400),
            (AssetId::new([3u8; 32]), 700),
        ]),
        None,
        None,
    );

    let TestContext {
        client,
        srv: _dont_drop,
        ..
    } = test_builder.finalize().await;

    let contract_balances = client
        .contract_balances(
            &contract_id,
            PaginationRequest {
                cursor: None,
                results: 3,
                direction,
            },
        )
        .await
        .unwrap();

    assert!(!contract_balances.results.is_empty());
    if direction == PageDirection::Forward {
        assert_eq!(contract_balances.results[0].amount, 1000);
        assert_eq!(contract_balances.results[1].amount, 400);
        assert_eq!(contract_balances.results[2].amount, 700);
    } else {
        assert_eq!(contract_balances.results[2].amount, 1000);
        assert_eq!(contract_balances.results[1].amount, 400);
        assert_eq!(contract_balances.results[0].amount, 700);
    }
}

fn key(i: u8) -> Bytes32 {
    Bytes32::new(
        [0u8; 31]
            .into_iter()
            .chain([i])
            .collect::<Vec<_>>()
            .try_into()
            .unwrap(),
    )
}

#[tokio::test]
async fn can_get_message_proof() {
    let config = Config::local_node();
    let coin = config
        .chain_conf
        .initial_state
        .as_ref()
        .unwrap()
        .coins
        .as_ref()
        .unwrap()
        .first()
        .unwrap()
        .clone();

    let slots_to_read = 2;

    let contract = vec![
        // Save the ptr to the script data to register 16.
        // Start db key
        op::gtf_args(0x10, 0x00, GTFArgs::ScriptData),
        // Set the location in memory to write the bytes to.
        op::movi(0x11, 100),
        op::aloc(0x11),
        op::move_(0x11, RegId::HP),
        op::movi(0x13, slots_to_read),
        // Write read to 0x11.
        // Write status to 0x30.
        // Get the db key the memory location in 0x10.
        // Read the number of slots in 0x13.
        op::srwq(0x11, 0x30, 0x10, 0x13),
        op::scwq(0x10, 0x31, 0x13),
        op::addi(0x14, 0x10, Bytes32::LEN.try_into().unwrap()),
        op::swwq(0x10, 0x32, 0x14, 0x13),
        op::srwq(0x11, 0x33, 0x10, 0x13),
        // Log out the data.
        op::log(0x30, 0x31, 0x32, 0x33),
        op::swwq(0x10, 0x30, 0x14, 0x13),
        op::scwq(0x10, 0x31, 0x13),
        op::log(0x30, 0x31, 0x00, 0x00),
        op::muli(0x15, 0x13, 32),
        op::logd(0x00, 0x00, 0x11, 0x15),
        // Return from the contract.
        op::ret(RegId::ONE),
    ];
    // Return.

    // Contract code.
    let bytecode: Witness = contract.into_iter().collect::<Vec<u8>>().into();

    // Setup the contract.
    let salt = Salt::zeroed();
    let contract = Contract::from(bytecode.as_ref());
    let root = contract.root();
    let state_root = Contract::initial_state_root(std::iter::empty());
    let id = contract.id(&salt, &root, &state_root);
    let output = Output::contract_created(id, state_root);

    // Create the contract deploy transaction.
    let mut contract_deploy = TransactionBuilder::create(bytecode, salt, vec![])
        .add_random_fee_input()
        .add_output(output)
        .finalize_as_transaction();

    let db_data = key(2)
        .as_ref()
        .iter()
        .copied()
        .chain(key(3).as_ref().iter().copied())
        .collect::<Vec<_>>();
    let script_data = key(1)
        .as_ref()
        .iter()
        .copied()
        .chain(db_data.clone().into_iter())
        .chain(Call::new(id, 0, 0).to_bytes())
        .collect();

    // Call contract script.
    // Save the ptr to the script data to register 16.
    // This will be used to read the contract id + two
    // empty params. So 32 + 8 + 8.
    let script = [
        op::gtf_args(0x10, 0x00, GTFArgs::ScriptData),
        op::addi(0x10, 0x10, (Bytes32::LEN * 3).try_into().unwrap()),
        // Call the contract and forward no coins.
        op::call(0x10, RegId::ZERO, RegId::ZERO, RegId::CGAS),
        // Return.
        op::ret(RegId::ONE),
    ];
    let script: Vec<u8> = script
        .iter()
        .flat_map(|op| u32::from(*op).to_be_bytes())
        .collect();

    let predicate = op::ret(RegId::ONE).to_bytes().to_vec();
    let owner = Input::predicate_owner(&predicate);
    let coin_input = Input::coin_predicate(
        Default::default(),
        owner,
        1000,
        coin.asset_id,
        TxPointer::default(),
        Default::default(),
        Default::default(),
        predicate,
        vec![],
    );

    // Set the contract input because we are calling a contract.
    let inputs = vec![
        Input::contract(
            UtxoId::new(Bytes32::zeroed(), 0),
            Bytes32::zeroed(),
            state_root,
            TxPointer::default(),
            id,
        ),
        coin_input,
    ];

    // The transaction will output a contract output and message output.
    let outputs = vec![Output::contract(0, Bytes32::zeroed(), Bytes32::zeroed())];

    // Create the contract calling script.
    let script = Transaction::script(
        1_000_000,
        script,
        script_data,
        policies::Policies::new().with_gas_price(0),
        inputs,
        outputs,
        vec![],
    );

    // setup server & client
    let srv = FuelService::new_node(config).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    client
        .estimate_predicates(&mut contract_deploy)
        .await
        .expect("Should be able to estimate deploy tx");

    // Deploy the contract.
    matches!(
        client.submit_and_await_commit(&contract_deploy).await,
        Ok(TransactionStatus::Success { .. })
    );

    let mut script = script.into();
    client
        .estimate_predicates(&mut script)
        .await
        .expect("Should be able to estimate deploy tx");
    // Call the contract.
    let tx_status = client.submit_and_await_commit(&script).await.unwrap();
    matches!(tx_status, TransactionStatus::Success { .. });

    let receipts = match tx_status {
        TransactionStatus::Success { receipts, .. } => Some(receipts),
        _ => None,
    };

    // Get the receipts from the contract call.
    let receipts = receipts.unwrap();
    let logd = receipts
        .iter()
        .find(|f| matches!(f, Receipt::LogData { .. }))
        .unwrap()
        .clone();
    let log = receipts
        .into_iter()
        .filter(|r| matches!(r, Receipt::Log { .. }))
        .collect::<Vec<_>>();
    assert_eq!(log[0].ra().unwrap(), 0);
    assert_eq!(log[0].rb().unwrap(), 0);
    assert_eq!(log[0].rc().unwrap(), slots_to_read as u64);
    assert_eq!(log[0].rd().unwrap(), 1);

    assert_eq!(log[1].ra().unwrap(), 0);
    assert_eq!(log[1].rb().unwrap(), 1);
    assert_eq!(logd.data().unwrap(), db_data);
}
