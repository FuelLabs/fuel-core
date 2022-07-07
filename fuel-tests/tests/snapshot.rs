use std::vec;

use fuel_core::database::Database;
use fuel_core::{
    config::{
        chain_config::{CoinConfig, ContractConfig, StateConfig},
        Config,
    },
    service::FuelService,
};
use fuel_core_interfaces::{
    common::{
        fuel_types::{Address, Bytes32, Salt},
        fuel_vm::prelude::{AssetId, Contract, ContractId, InterpreterStorage, Storage},
    },
    model::BlockHeight,
};

#[tokio::test]
async fn snapshot_state_config() {
    let mut db = Database::default();

    let owner = Address::default();
    let asset_id = AssetId::new([1u8; 32]);

    // Extract later for a test case
    let contract = Contract::default();
    let id = ContractId::new([12; 32]);

    // setup config
    let mut config = Config::local_node();
    let starting_state = StateConfig {
        height: Some(BlockHeight::from(10u64)),
        contracts: Some(vec![ContractConfig {
            code: vec![8; 32],
            salt: Salt::new([9; 32]),
            state: Some(vec![
                (Bytes32::new([5u8; 32]), Bytes32::new([8u8; 32])),
                (Bytes32::new([5u8; 32]), Bytes32::new([9u8; 32])),
            ]),
            balances: Some(vec![
                (AssetId::new([3u8; 32]), 100),
                (AssetId::new([10u8; 32]), 10000),
            ]),
        }]),
        coins: Some(
            vec![
                (owner, 50, asset_id),
                (owner, 100, asset_id),
                (owner, 150, asset_id),
            ]
            .into_iter()
            .map(|(owner, amount, asset_id)| CoinConfig {
                tx_id: Some(Bytes32::new([0; 32])),
                output_index: Some(0),
                block_created: Some(BlockHeight::from(0u64)),
                maturity: Some(BlockHeight::from(0u64)),
                owner,
                amount,
                asset_id,
            })
            .collect(),
        ),
    };

    config.chain_conf.initial_state = Some(starting_state.clone());

    Storage::<ContractId, Contract>::insert(&mut db, &id, &contract).unwrap();

    InterpreterStorage::storage_contract_root_insert(
        &mut db,
        &id,
        &Salt::new([5; 32]),
        &Bytes32::new([0; 32]),
    )
    .unwrap();

    // setup server & client
    let _ = FuelService::from_database(db.clone(), config)
        .await
        .unwrap();

    let state_conf = StateConfig::generate_state_config(db);

    //assert_eq!(state_conf.coins, starting_state.coins);

    assert_eq!(state_conf.height, starting_state.height);

    println!("{:?}", starting_state.contracts);
    println!("{:?}", state_conf.contracts.unwrap()[1]);
}
