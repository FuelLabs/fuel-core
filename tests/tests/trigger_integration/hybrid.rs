use fuel_core::{
    database::Database,
    service::{
        Config,
        FuelService,
    },
};
use fuel_core_client::client::{
    FuelClient,
    PageDirection,
    PaginationRequest,
};
use fuel_core_poa::Trigger;
use fuel_core_types::{
    fuel_asm::*,
    fuel_crypto::SecretKey,
    fuel_tx::{
        Finalizable,
        TransactionBuilder,
    },
    secrecy::Secret,
};
use rand::{
    rngs::StdRng,
    Rng,
    SeedableRng,
};
use std::time::Duration;

#[tokio::test(start_paused = true)]
async fn poa_hybrid_produces_empty_blocks_at_correct_rate() {
    let rounds = 64;
    let round_time_seconds = 30;

    let mut rng = StdRng::seed_from_u64(10);

    let db = Database::default();
    let mut config = Config::local_node();
    config.consensus_key = Some(Secret::new(SecretKey::random(&mut rng).into()));
    config.block_production = Trigger::Hybrid {
        max_block_time: Duration::new(round_time_seconds, 0),
        max_tx_idle_time: Duration::new(5, 0),
        min_block_time: Duration::new(2, 0),
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

        // Make sure that we do not depend on scheduler behavior too much
        for _ in 0..(rng.gen::<u8>() % 8) {
            tokio::task::yield_now().await;
        }

        let count_now = resp.results.len();
        if count_now > count_start + rounds {
            break
        }
    }

    let time_end = tokio::time::Instant::now();

    // Require at least minimum time, allow up to one round time of error
    let secs_per_round = (time_end - time_start).as_secs() / (rounds as u64);
    assert!(
        round_time_seconds <= secs_per_round
            && secs_per_round
                <= round_time_seconds + 2 * (rounds as u64) / round_time_seconds,
        "Round time not within treshold"
    );
}

#[tokio::test(start_paused = true)]
async fn poa_hybrid_produces_nonempty_blocks_at_correct_rate() {
    let rounds = 64;
    let round_time_seconds = 30;

    let mut rng = StdRng::seed_from_u64(10);

    let db = Database::default();
    let mut config = Config::local_node();
    config.consensus_key = Some(Secret::new(SecretKey::random(&mut rng).into()));
    config.block_production = Trigger::Hybrid {
        max_block_time: Duration::new(30, 0),
        max_tx_idle_time: Duration::new(5, 0),
        min_block_time: Duration::new(round_time_seconds, 0),
    };

    let srv = FuelService::from_database(db.clone(), config)
        .await
        .unwrap();

    let client = FuelClient::from(srv.bound_address);

    for i in 0..200 {
        let mut tx =
            TransactionBuilder::script([op::movi(0x10, i)].into_iter().collect(), vec![]);
        let _tx_id = client.submit(&tx.finalize().into()).await.unwrap();
    }

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

        // Make sure that we do not depend on scheduler behavior too much
        for _ in 0..(rng.gen::<u8>() % 8) {
            tokio::task::yield_now().await;
        }

        let count_now = resp.results.len();
        if dbg!(count_now) > dbg!(count_start + rounds) {
            break
        }
    }

    let time_end = tokio::time::Instant::now();

    // Require at least minimum time, allow up to one round time of error
    let secs_per_round = (time_end - time_start).as_secs() / (rounds as u64);
    assert!(
        round_time_seconds <= secs_per_round
            && secs_per_round
                <= round_time_seconds + 2 * (rounds as u64) / round_time_seconds,
        "Round time not within treshold"
    );
}
