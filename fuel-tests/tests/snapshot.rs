use fuel_core::chain_config::StateConfig;
use fuel_core::database::Database;
use fuel_core::{
    chain_config::{CoinConfig, ContractConfig},
    service::{Config, FuelService},
};
use fuel_tx::AssetId;
use fuel_types::{Bytes32, Salt};
use fuel_vm::prelude::Address;

#[tokio::test]
async fn snapshot_chain_config() {
    let mut db = Database::default();

    let owner = Address::default();
    let asset_id = AssetId::new([1u8; 32]);

    // Extract later for a test case
    let contract = fuel_vm::prelude::Contract::default();
    let id = fuel_types::ContractId::new([12; 32]);

    // setup config
    let mut config = Config::local_node();
    config.chain_conf.initial_state = Some(StateConfig {
        height: None,
        contracts: Some(vec![ContractConfig {
            code: vec![8; 32],
            salt: Salt::new([9; 32]),
            state: None,
            balances: None,
        }]),
        coins: Some(
            vec![
                (owner, 50, asset_id),
                (owner, 100, asset_id),
                (owner, 150, asset_id),
            ]
            .into_iter()
            .map(|(owner, amount, asset_id)| CoinConfig {
                tx_id: None,
                output_index: None,
                block_created: None,
                maturity: None,
                owner,
                amount,
                asset_id,
            })
            .collect(),
        ),
    });

    fuel_storage::Storage::<fuel_types::ContractId, fuel_vm::prelude::Contract>::insert(
        &mut db, &id, &contract,
    )
    .unwrap();

    fuel_vm::storage::InterpreterStorage::storage_contract_root_insert(
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

    assert!(state_conf.contracts.is_some());
    assert!(state_conf.coins.is_some());
}
