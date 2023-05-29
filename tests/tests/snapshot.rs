use fuel_core::{
    chain_config::{
        CoinConfig,
        ContractConfig,
        MessageConfig,
        StateConfig,
    },
    database::Database,
    service::{
        Config,
        FuelService,
    },
};
use fuel_core_types::{
    blockchain::primitives::DaBlockHeight,
    fuel_types::{
        BlockHeight,
        Nonce,
        *,
    },
};
use rand::{
    rngs::StdRng,
    Rng,
    SeedableRng,
};

#[tokio::test]
async fn snapshot_state_config() {
    let mut rng = StdRng::seed_from_u64(1234);
    let db = Database::default();

    let owner = Address::default();

    // setup config
    let mut config = Config::local_node();
    let starting_state = StateConfig {
        height: Some(BlockHeight::from(10)),
        contracts: Some(vec![ContractConfig {
            contract_id: [11; 32].into(),
            code: vec![8; 32],
            salt: Salt::new([9; 32]),
            state: Some(vec![
                (Bytes32::new([5u8; 32]), Bytes32::new([8u8; 32])),
                (Bytes32::new([7u8; 32]), Bytes32::new([9u8; 32])),
            ]),
            balances: Some(vec![
                (AssetId::new([3u8; 32]), 100),
                (AssetId::new([10u8; 32]), 10000),
            ]),
            tx_id: Some(rng.gen()),
            output_index: Some(rng.gen()),
            tx_pointer_block_height: Some(BlockHeight::from(10)),
            tx_pointer_tx_idx: Some(rng.gen()),
        }]),
        coins: Some(
            vec![
                (owner, 50, AssetId::new([8u8; 32])),
                (owner, 100, AssetId::new([3u8; 32])),
                (owner, 150, AssetId::new([5u8; 32])),
            ]
            .into_iter()
            .map(|(owner, amount, asset_id)| CoinConfig {
                tx_id: None,
                output_index: None,
                tx_pointer_block_height: Some(Default::default()),
                tx_pointer_tx_idx: Some(0),
                maturity: Some(Default::default()),
                owner,
                amount,
                asset_id,
            })
            .collect(),
        ),
        messages: Some(vec![MessageConfig {
            sender: rng.gen(),
            recipient: rng.gen(),
            nonce: Nonce::from(rng.gen_range(0..1000)),
            amount: rng.gen_range(0..1000),
            data: vec![],
            da_height: DaBlockHeight(rng.gen_range(0..1000)),
        }]),
    };

    config.chain_conf.initial_state = Some(starting_state.clone());

    // setup server & client
    let _ = FuelService::from_database(db.clone(), config)
        .await
        .unwrap();

    let state_conf = StateConfig::generate_state_config(db).unwrap();

    // initial state

    let starting_coin = starting_state.clone().coins.unwrap();

    let state_coin = state_conf.clone().coins.unwrap();

    for i in 0..starting_coin.len() {
        // all values are checked except tx_id and output_index as those are generated and not
        // known at initialization
        assert_eq!(state_coin[i].owner, starting_coin[i].owner);
        assert_eq!(state_coin[i].asset_id, starting_coin[i].asset_id);
        assert_eq!(state_coin[i].amount, starting_coin[i].amount);
        assert_eq!(
            state_coin[i].tx_pointer_block_height,
            starting_coin[i].tx_pointer_block_height
        );
        assert_eq!(state_coin[i].maturity, starting_coin[i].maturity);
    }

    assert_eq!(state_conf.height, starting_state.height);

    assert_eq!(state_conf.contracts, starting_state.contracts);

    assert_eq!(state_conf.messages, starting_state.messages)
}
