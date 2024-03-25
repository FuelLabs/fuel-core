use fuel_core::{
    chain_config::{
        Randomize,
        SnapshotReader,
        StateConfig,
    },
    combined_database::CombinedDatabase,
    service::{
        Config,
        FuelService,
    },
};
use fuel_core_types::blockchain::primitives::DaBlockHeight;
use rand::{
    rngs::StdRng,
    SeedableRng,
};

#[tokio::test]
async fn loads_snapshot() {
    let mut rng = StdRng::seed_from_u64(1234);
    let db = CombinedDatabase::default();

    // setup config
    let starting_state = StateConfig {
        block_height: u32::MAX.into(),
        da_block_height: DaBlockHeight(u64::MAX),
        ..StateConfig::randomize(&mut rng)
    };
    let config = Config {
        snapshot_reader: SnapshotReader::local_testnet()
            .with_state_config(starting_state.clone()),
        ..Config::local_node()
    };

    // setup server & client
    let _ = FuelService::from_combined_database(db.clone(), config)
        .await
        .unwrap();

    let stored_state = db.read_state_config().unwrap();

    // initial state
    pretty_assertions::assert_eq!(starting_state.sorted(), stored_state);
}
