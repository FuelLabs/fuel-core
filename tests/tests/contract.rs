use crate::helpers::{
    TestContext,
    TestSetupBuilder,
};
use fuel_core::service::{
    Config,
    FuelService,
};
use fuel_core_client::client::{
    types::TransactionStatus,
    FuelClient,
    PageDirection,
    PaginationRequest,
};
use fuel_core_types::{
    fuel_asm::*,
    fuel_tx::*,
    fuel_types::bytes::*,
    fuel_vm::*,
};
use rstest::rstest;

const SEED: u64 = 2322;

#[tokio::test]
async fn test_contract_salt() {
    let mut test_builder = TestSetupBuilder::new(SEED);
    let (_, contract_id) = test_builder.setup_contract(vec![], None, None, None);

    // spin up node
    let TestContext {
        client,
        srv: _dont_drop,
        ..
    } = test_builder.finalize().await;

    let contract = client
        .contract(format!("{contract_id:#x}").as_str())
        .await
        .unwrap();

    // Check that salt is 0x Hex prefixed
    let salt = contract.unwrap().salt;
    assert_eq!("0x", &salt.to_string()[..2]);
}

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
        .contract_balance(
            format!("{contract_id:#x}").as_str(),
            Some(format!("{asset:#x}").as_str()),
        )
        .await
        .unwrap();

    assert_eq!(balance, test_balance);
}

#[rstest]
#[tokio::test]
async fn test_5_contract_balances(
    #[values(PageDirection::Forward)] direction: PageDirection,
    // #[values(PageDirection::Forward, PageDirection::Backward)] direction: PageDirection,
    // Rocksdb doesn't support reverse seeks using a prefix, we'd need to implement a custom
    // comparator to support this usecase.
    // > One common bug of using prefix iterating is to use prefix mode to iterate in reverse order. But it is not yet supported.
    // https://github.com/facebook/rocksdb/wiki/Prefix-Seek#limitation
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
            format!("{contract_id:#x}").as_str(),
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
        assert_eq!(contract_balances.results[0].amount.0, 1000);
        assert_eq!(contract_balances.results[1].amount.0, 400);
        assert_eq!(contract_balances.results[2].amount.0, 700);
    } else {
        assert_eq!(contract_balances.results[2].amount.0, 1000);
        assert_eq!(contract_balances.results[1].amount.0, 400);
        assert_eq!(contract_balances.results[0].amount.0, 700);
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

    let contract = vec![
        // Save the ptr to the script data to register 16.
        // Start db key
        op::gtf_args(0x10, 0x00, GTFArgs::ScriptData),
        // Set the location in memory to write the bytes to.
        op::movi(0x11, 100),
        op::aloc(0x11),
        op::addi(0x11, RegId::HP, 1),
        op::movi(0x13, 2),
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
    let contract_deploy = TransactionBuilder::create(bytecode, salt, vec![])
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
    let script = vec![
        // Save the ptr to the script data to register 16.
        // This will be used to read the contract id + two
        // empty params. So 32 + 8 + 8.
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
    let owner = Input::predicate_owner(&predicate, &ConsensusParameters::DEFAULT);
    let coin_input = Input::coin_predicate(
        Default::default(),
        owner,
        1000,
        coin.asset_id,
        TxPointer::default(),
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
    let outputs = vec![Output::Contract {
        input_index: 0,
        balance_root: Bytes32::zeroed(),
        state_root: Bytes32::zeroed(),
    }];

    // Create the contract calling script.
    let script = Transaction::script(
        Default::default(),
        1_000_000,
        Default::default(),
        script,
        script_data,
        inputs,
        outputs,
        vec![],
    );

    let transaction_id = script.id(&ConsensusParameters::DEFAULT);

    // setup server & client
    let srv = FuelService::new_node(config).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    // Deploy the contract.
    matches!(
        client.submit_and_await_commit(&contract_deploy).await,
        Ok(TransactionStatus::Success { .. })
    );

    // Call the contract.
    matches!(
        client.submit_and_await_commit(&script.into()).await,
        Ok(TransactionStatus::Success { .. })
    );

    // Get the receipts from the contract call.
    let receipts = client
        .receipts(transaction_id.to_string().as_str())
        .await
        .unwrap()
        .unwrap();
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
    assert_eq!(log[0].rc().unwrap(), 0);
    assert_eq!(log[0].rd().unwrap(), 1);

    assert_eq!(log[1].ra().unwrap(), 1);
    assert_eq!(log[1].rb().unwrap(), 1);
    assert_eq!(logd.data().unwrap(), db_data);
}
