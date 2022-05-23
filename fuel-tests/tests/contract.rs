use crate::helpers::{TestContext, TestSetupBuilder};
use fuel_crypto::SecretKey;
use fuel_tx::TransactionBuilder;
use fuel_vm::{consts::*, prelude::*};
use itertools::Itertools;
use rand::{rngs::StdRng, Rng, SeedableRng};

#[tokio::test]
async fn test_contract_salt() {
    let mut rng = StdRng::seed_from_u64(2322);
    let mut test_builder = TestSetupBuilder::new(2322);
    let (_, contract_id) = test_builder.setup_contract(vec![]);


    // spin up node
    let TestContext { client, .. } = test_builder.finalize().await;

    let contract = client
        .contract(format!("{:#x}", contract_id).as_str())
        .await
        .unwrap();

    // Check that salt is 0x Hex prefixed
    let salt = contract.unwrap().salt;
    assert_eq!("0x", &salt.to_string()[..2]);
}

#[tokio::test]
async fn test_contract_balance_default() {
    let mut rng = StdRng::seed_from_u64(2322);
    let mut test_builder = TestSetupBuilder::new(2322);
    let (_, contract_id) = test_builder.setup_contract(vec![]);


    // spin up node
    let TestContext { client, .. } = test_builder.finalize().await;

    let asset_id = AssetId::new([1u8; 32]);

    let balance = client
        .contract_balance(
            format!("{:#x}", contract_id).as_str(),
            Some(format!("{:#x}", asset_id).as_str()),
        )
        .await
        .unwrap();

    assert_eq!(balance, 0); // Ensures default value is working properly
}

#[tokio::test]
async fn test_contract_balance() {
    let mut rng = StdRng::seed_from_u64(2322);
    let mut test_builder = TestSetupBuilder::new(2322);
    let (_, contract_id) = test_builder.setup_funded_contract(vec![]);

    // initialize 10 random transactions that transfer coins and call a contract
    let transactions = (1..=10)
        .into_iter()
        .map(|i| {
            let secret = SecretKey::random(&mut rng);
            TransactionBuilder::script(
                Opcode::RET(REG_ONE).to_bytes().into_iter().collect(),
                vec![],
            )
            .gas_limit(100)
            .gas_price(1)
            .byte_price(1)
            .add_unsigned_coin_input(
                rng.gen(),
                &secret,
                1000 + i,
                Default::default(),
                0,
                vec![],
                vec![],
            )
            .add_input(Input::Contract {
                utxo_id: Default::default(),
                balance_root: Default::default(),
                state_root: Default::default(),
                contract_id,
            })
            .add_output(Output::Change {
                amount: 0,
                asset_id: Default::default(),
                to: rng.gen(),
            })
            .add_output(Output::Contract {
                input_index: 1,
                balance_root: Default::default(),
                state_root: Default::default(),
            })
            .finalize()
        })
        .collect_vec();

    // setup genesis block with coins that transactions can spend
    test_builder.config_coin_inputs_from_transactions(&transactions);

    // spin up node
    let TestContext { client, .. } = test_builder.finalize().await;

    let asset_id = AssetId::new([1u8; 32]);

    let balance = client
        .contract_balance(
            format!("{:#x}", contract_id).as_str(),
            Some(format!("{:#x}", asset_id).as_str()),
        )
        .await
        .unwrap();

    assert_eq!(balance, 500);
}
