use fuel_core::{
    database::Database,
    service::{
        Config,
        FuelService,
    },
};
use fuel_core_client::client::{
    pagination::{
        PageDirection,
        PaginationRequest,
    },
    FuelClient,
};
use fuel_core_poa::{
    signer::SignMode,
    Trigger,
};
use fuel_core_types::{
    fuel_asm::*,
    fuel_crypto::SecretKey,
    fuel_tx::TransactionBuilder,
    secrecy::Secret,
};
use rand::{
    rngs::StdRng,
    Rng,
    SeedableRng,
};
use std::time::Duration;

#[tokio::test(start_paused = true)]
async fn poa_interval_produces_empty_blocks_at_correct_rate() {
    let rounds = 64;
    let round_time_seconds = 2;

    let mut rng = StdRng::seed_from_u64(10);

    let db = Database::default();
    let mut config = Config::local_node();
    config.graphql_config.max_queries_complexity = 1_000_000;
    config.consensus_signer =
        SignMode::Key(Secret::new(SecretKey::random(&mut rng).into()));
    config.block_production = Trigger::Interval {
        block_time: Duration::new(round_time_seconds, 0),
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
        round_time_seconds <= secs_per_round
            && secs_per_round
                <= round_time_seconds + 2 * (rounds as u64) / round_time_seconds,
        "Round time not within threshold"
    );
}

#[tokio::test(start_paused = true)]
async fn poa_interval_produces_nonempty_blocks_at_correct_rate() {
    let rounds = 64;
    let round_time_seconds = 2;
    let tx_count = 100;

    let mut rng = StdRng::seed_from_u64(10);

    let db = Database::default();
    let mut config = Config::local_node();
    config.graphql_config.max_queries_complexity = 1_000_000;
    config.consensus_signer =
        SignMode::Key(Secret::new(SecretKey::random(&mut rng).into()));
    config.block_production = Trigger::Interval {
        block_time: Duration::new(round_time_seconds, 0),
    };

    let srv = FuelService::from_database(db.clone(), config)
        .await
        .unwrap();

    let client = FuelClient::from(srv.bound_address);

    for i in 0..tx_count {
        let tx = TransactionBuilder::script(
            [op::movi(0x10, i.try_into().unwrap())]
                .into_iter()
                .collect(),
            vec![],
        )
        .add_unsigned_coin_input(
            SecretKey::random(&mut rng),
            rng.gen(),
            rng.gen(),
            rng.gen(),
            Default::default(),
        )
        .finalize_as_transaction();
        let _tx_id = client.submit(&tx).await.unwrap();
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
        "Round time not within threshold"
    );

    // Make sure all txs got produced
    let mut blocks = client
        .blocks(PaginationRequest {
            cursor: None,
            results: 1024,
            direction: PageDirection::Forward,
        })
        .await
        .expect("blocks request failed")
        .results;
    // Remove the genesis block because it doesn't contain transactions
    blocks.remove(0);
    let blocks_without_genesis = blocks;

    let txs_len: usize = blocks_without_genesis
        .iter()
        .map(|block| block.transactions.len())
        .sum();
    // Each block(except genesis block) contains at least 1 coinbase transaction
    let coinbase_tx_count = blocks_without_genesis.len();

    assert_eq!(txs_len, coinbase_tx_count + tx_count);
}
