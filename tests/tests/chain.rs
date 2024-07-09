use fuel_core::{
    chain_config::{
        ChainConfig,
        CoinConfig,
        StateConfig,
    },
    service::{
        Config,
        FuelService,
    },
};
use fuel_core_client::client::{
    types::{
        primitives::{
            AssetId,
            UtxoId,
        },
        TransactionStatus,
    },
    FuelClient,
};
use fuel_core_types::{
    fuel_crypto::SecretKey,
    fuel_tx::{
        Input,
        Output,
        TransactionBuilder,
    },
    fuel_types::ChainId,
};
use rand::SeedableRng;

#[tokio::test]
async fn chain_info() {
    let node_config = Config::local_node();
    let srv = FuelService::new_node(node_config.clone()).await.unwrap();
    let client = FuelClient::from(srv.bound_address);

    let chain_info = client.chain_info().await.unwrap();

    assert_eq!(0, chain_info.da_height);
    let chain_config = node_config.snapshot_reader.chain_config();
    assert_eq!(chain_config.chain_name, chain_info.name);
    assert_eq!(
        chain_config.consensus_parameters,
        chain_info.consensus_parameters.clone()
    );

    assert_eq!(
        chain_config.consensus_parameters.gas_costs(),
        chain_info.consensus_parameters.gas_costs()
    );
}

#[tokio::test]
async fn network_operates_with_non_zero_chain_id() {
    let mut rng = rand::rngs::StdRng::seed_from_u64(0xBAADF00D);
    let secret = SecretKey::random(&mut rng);
    let amount = 10000;
    let owner = Input::owner(&secret.public_key());
    let utxo_id = UtxoId::new([1; 32].into(), 0);

    let state_config = StateConfig {
        coins: vec![CoinConfig {
            tx_id: *utxo_id.tx_id(),
            output_index: utxo_id.output_index(),
            owner,
            amount,
            asset_id: AssetId::BASE,
            ..Default::default()
        }],
        ..Default::default()
    };
    let mut chain_config = ChainConfig::local_testnet();

    // Given
    let chain_id = ChainId::new(0xDEAD);
    chain_config.consensus_parameters.set_chain_id(chain_id);
    let node_config = Config {
        debug: true,
        utxo_validation: true,
        ..Config::local_node_with_configs(chain_config, state_config)
    };

    let srv = FuelService::new_node(node_config.clone()).await.unwrap();
    let client = FuelClient::from(srv.bound_address);
    let script = TransactionBuilder::script(vec![], vec![])
        .with_chain_id(chain_id)
        .max_fee_limit(amount)
        .add_unsigned_coin_input(
            secret,
            utxo_id,
            amount,
            AssetId::BASE,
            Default::default(),
        )
        .add_output(Output::Change {
            to: owner,
            amount,
            asset_id: AssetId::BASE,
        })
        .finalize_as_transaction();

    // When
    let result = client
        .submit_and_await_commit(&script)
        .await
        .expect("transaction should insert");

    // Then
    assert!(matches!(result, TransactionStatus::Success { .. }))
}

#[tokio::test]
async fn network_operates_with_non_zero_base_asset_id() {
    let mut rng = rand::rngs::StdRng::seed_from_u64(0xBAADF00D);
    let secret = SecretKey::random(&mut rng);
    let amount = 10000;
    let starting_gas_price = 1;
    let owner = Input::owner(&secret.public_key());
    let utxo_id = UtxoId::new([1; 32].into(), 0);

    // Given
    let new_base_asset_id = AssetId::new([6; 32]);

    let state_config = StateConfig {
        coins: vec![CoinConfig {
            tx_id: *utxo_id.tx_id(),
            output_index: utxo_id.output_index(),
            owner,
            amount,
            asset_id: new_base_asset_id,
            ..Default::default()
        }],
        ..Default::default()
    };
    let mut chain_config = ChainConfig::local_testnet();
    chain_config
        .consensus_parameters
        .set_base_asset_id(new_base_asset_id);
    let node_config = Config {
        debug: true,
        utxo_validation: true,
        starting_gas_price,
        ..Config::local_node_with_configs(chain_config, state_config)
    };

    let srv = FuelService::new_node(node_config.clone()).await.unwrap();
    let client = FuelClient::from(srv.bound_address);
    let script = TransactionBuilder::script(vec![], vec![])
        .max_fee_limit(amount)
        .add_unsigned_coin_input(
            secret,
            utxo_id,
            amount,
            new_base_asset_id,
            Default::default(),
        )
        .add_output(Output::Change {
            to: owner,
            amount,
            asset_id: new_base_asset_id,
        })
        .finalize_as_transaction();

    // When
    let result = client
        .submit_and_await_commit(&script)
        .await
        .expect("transaction should insert");

    // Then
    let expected_fee = 1;
    assert!(matches!(result, TransactionStatus::Success { .. }));
    let balance = client
        .balance(&owner, Some(&new_base_asset_id))
        .await
        .expect("Should fetch the balance");
    assert_eq!(balance, amount - expected_fee);
}
