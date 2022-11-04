use fuel_core::{
    chain_config::BlockProduction,
    database::Database,
    service::{
        Config,
        FuelService,
    },
};
use fuel_core_interfaces::{
    common::{
        fuel_tx::Transaction,
        secrecy::Secret,
    },
    model::FuelBlockConsensus,
};
use fuel_gql_client::{
    client::{
        types::TransactionStatus,
        FuelClient,
        PageDirection,
        PaginationRequest,
    },
    fuel_types::Bytes32,
    prelude::SecretKey,
};
use rand::{
    rngs::StdRng,
    SeedableRng,
};
use std::{
    str::FromStr,
    time::Duration,
};

#[tokio::test(start_paused = true)]
async fn poa_hybrid_produces_empty_blocks_at_correct_rate() {
    let rounds = 64;
    let round_time_seconds = 30;

    let mut rng = StdRng::seed_from_u64(10);

    let db = Database::default();
    let mut config = Config::local_node();
    config.consensus_key = Some(Secret::new(SecretKey::random(&mut rng).into()));
    config.chain_conf.block_production = BlockProduction::ProofOfAuthority {
        trigger: fuel_poa_coordinator::Trigger::Hybrid {
            max_block_time: Duration::new(round_time_seconds, 0),
            max_tx_idle_time: Duration::new(5, 0),
            min_block_time: Duration::new(2, 0),
        },
    };

    let srv = FuelService::from_database(db.clone(), config)
        .await
        .unwrap();

    let client = FuelClient::from(srv.bound_address);

    let time_start = tokio::time::Instant::now();
    let count_start = client
        .blocks(PaginationRequest {
            cursor: None,
            results: 1024,
            direction: PageDirection::Forward,
        })
        .await
        .expect("blocks request failed")
        .results
        .len();

    loop {
        let resp = client
            .blocks(PaginationRequest {
                cursor: None,
                results: 1024,
                direction: PageDirection::Forward,
            })
            .await
            .expect("blocks request failed");

        let count_now = resp.results.len();

        if count_now > count_start + rounds {
            break
        }
    }

    let time_end = tokio::time::Instant::now();

    // Require at least minimum time, allow up to one round time of error
    let secs_per_round = (time_end - time_start).as_secs() / (rounds as u64);
    assert!(
        30 <= secs_per_round
            && secs_per_round <= 30 + (rounds as u64) / round_time_seconds,
        "Round time not within treshold"
    );
}
