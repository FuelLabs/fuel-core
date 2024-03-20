use fuel_core::{
    chain_config::{
        Randomize,
        SnapshotReader,
        StateConfig,
        TableEntry,
    },
    database::{
        ChainStateDb,
        Database,
    },
    query::BlockQueryData,
    service::{
        Config,
        FuelService,
    },
};
use fuel_core_storage::{
    blueprint::BlueprintInspect,
    column::Column,
    iter::IterDirection,
    structured_storage::TableWithBlueprint,
};
use fuel_core_types::blockchain::primitives::DaBlockHeight;
use rand::{
    rngs::StdRng,
    SeedableRng,
};

#[tokio::test]
async fn loads_snapshot() {
    let mut rng = StdRng::seed_from_u64(1234);
    let db = Database::default();

    // setup config
    let starting_state = StateConfig {
        block_height: u32::MAX.into(),
        da_block_height: DaBlockHeight(u64::MAX),
        ..StateConfig::randomize(&mut rng)
    };
    let config = Config {
        state_reader: SnapshotReader::in_memory(starting_state.clone()),
        ..Config::local_node()
    };

    // setup server & client
    let _ = FuelService::from_database(db.clone(), config)
        .await
        .unwrap();

    fn get_entries<T>(db: &Database) -> Vec<TableEntry<T>>
    where
        T: TableWithBlueprint<Column = Column>,
        T::Blueprint: BlueprintInspect<T, Database>,
    {
        use itertools::Itertools;
        db.entries(None, IterDirection::Forward)
            .try_collect()
            .unwrap()
    }

    let block = db.latest_block().unwrap();
    let stored_state = StateConfig::from_tables(
        get_entries(&db),
        get_entries(&db),
        get_entries(&db),
        get_entries(&db),
        get_entries(&db),
        get_entries(&db),
        block.header().da_height,
        *block.header().height(),
    );

    // initial state
    pretty_assertions::assert_eq!(starting_state.sorted(), stored_state);
}
