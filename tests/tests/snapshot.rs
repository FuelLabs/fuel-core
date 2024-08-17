use fuel_core::{
    chain_config::{
        LastBlockConfig,
        Randomize,
        StateConfig,
    },
    combined_database::CombinedDatabase,
    service::{
        Config,
        FuelService,
    },
};
use fuel_core_poa::{
    ports::Database,
    Trigger,
};
use fuel_core_storage::transactional::AtomicView;
use fuel_core_types::blockchain::primitives::DaBlockHeight;
use rand::{
    rngs::StdRng,
    Rng,
    SeedableRng,
};

#[tokio::test]
async fn loads_snapshot() {
    let mut rng = StdRng::seed_from_u64(1234);
    let db = CombinedDatabase::default();
    let blocks_root = rng.gen();

    // setup config
    let starting_state = StateConfig {
        last_block: Some(LastBlockConfig {
            block_height: (u32::MAX - 1).into(),
            da_block_height: DaBlockHeight(u64::MAX),
            consensus_parameters_version: u32::MAX - 1,
            state_transition_version: u32::MAX - 1,
            blocks_root,
        }),
        ..StateConfig::randomize(&mut rng)
    };
    // Disable block production
    let mut config = Config::local_node_with_state_config(starting_state.clone());
    config.debug = false;
    config.block_production = Trigger::Never;

    // setup server & client
    let _ = FuelService::from_combined_database(db.clone(), config)
        .await
        .unwrap();

    let actual_state = db.read_state_config().unwrap();
    let mut expected = starting_state.sorted();
    expected.last_block = Some(LastBlockConfig {
        block_height: u32::MAX.into(),
        da_block_height: DaBlockHeight(u64::MAX),
        consensus_parameters_version: u32::MAX,
        state_transition_version: u32::MAX,
        blocks_root: db
            .on_chain()
            .latest_view()
            .unwrap()
            .block_header_merkle_root(&u32::MAX.into())
            .unwrap(),
    });

    // initial state
    pretty_assertions::assert_eq!(expected, actual_state);
}
