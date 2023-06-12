use fuel_core::{
    database::Database,
    service::{
        Config,
        FuelService,
    },
};
use fuel_core_client::client::{
    types::TransactionStatus,
    FuelClient,
};
use fuel_core_p2p::{
    Multiaddr,
    PeerId,
};
use fuel_core_poa::Trigger;
use fuel_core_types::{
    blockchain::{
        consensus::Consensus,
        primitives::BlockId,
    },
    fuel_crypto::SecretKey,
    fuel_tx::Transaction,
    secrecy::Secret,
};
use rand::{
    rngs::StdRng,
    SeedableRng,
};
use std::{
    str::FromStr,
    time::Duration,
};

#[tokio::test]
async fn can_get_sealed_block_from_poa_produced_block() {
    let mut rng = StdRng::seed_from_u64(10);
    let poa_secret = SecretKey::random(&mut rng);
    let poa_public = poa_secret.public_key();

    let db = Database::default();
    let mut config = Config::local_node();
    config.consensus_key = Some(Secret::new(poa_secret.into()));
    let srv = FuelService::from_database(db.clone(), config)
        .await
        .unwrap();
    let client = FuelClient::from(srv.bound_address);

    let status = client
        .submit_and_await_commit(&Transaction::default_test_tx())
        .await
        .unwrap();

    let block_id = match status {
        TransactionStatus::Success { block_id, .. } => block_id,
        _ => {
            panic!("unexpected result")
        }
    };

    let block_id = BlockId::from_str(&block_id).unwrap();

    // check sealed block header is correct
    let sealed_block_header = db
        .get_sealed_block_header(&block_id)
        .unwrap()
        .expect("expected sealed header to be available");

    // verify signature
    let block_id = sealed_block_header.entity.id();
    let signature = match sealed_block_header.consensus {
        Consensus::PoA(poa) => poa.signature,
        _ => panic!("Not expected consensus"),
    };
    signature
        .verify(&poa_public, &block_id.into_message())
        .expect("failed to verify signature");

    // check sealed block is correct
    let sealed_block = db
        .get_sealed_block_by_id(&block_id)
        .unwrap()
        .expect("expected sealed header to be available");

    // verify signature
    let block_id = sealed_block.entity.id();
    let signature = match sealed_block.consensus {
        Consensus::PoA(poa) => poa.signature,
        _ => panic!("Not expected consensus"),
    };
    signature
        .verify(&poa_public, &block_id.into_message())
        .expect("failed to verify signature");
}

// Starts first_producer which creates some blocks
// Then starts second_producer that uses the first one as a reserved peer.
// second_producer should not produce blocks while the first one is producing
// after the first_producer stops, second_producer should start producing blocks
#[tokio::test]
async fn test_poa_multiple_producers() {
    use fuel_core_p2p::config::convert_to_libp2p_keypair;
    use fuel_core_poa::ports::BlockImporter;
    use futures::StreamExt;

    // given the first producer
    let mut rng = StdRng::seed_from_u64(10);
    let poa_secret = SecretKey::random(&mut rng);

    let db = Database::default();
    let mut config = Config::local_node();
    let p2p_keypair = convert_to_libp2p_keypair(&mut poa_secret.to_vec()).unwrap();
    config.consensus_key = Some(Secret::new(poa_secret.into()));
    config.block_production = Trigger::Interval {
        block_time: Duration::from_secs(3),
    };
    match config.p2p {
        Some(ref mut p2p) => {
            p2p.keypair = p2p_keypair.clone();
            p2p.enable_mdns = true;
        }
        None => panic!("expected p2p config"),
    }

    let first_producer = FuelService::from_database(db.clone(), config.clone())
        .await
        .unwrap();

    // assert that we are producing blocks
    let mut first_five = first_producer.shared.block_importer.block_stream().take(5);
    while let Some(_) = first_five.next().await {}

    // using its Multiaddr to setup the second producer
    let first_producer_multi_addr: Multiaddr = {
        let ip_addr = first_producer.bound_address.ip();
        let port: u16 = first_producer.bound_address.port();
        let peer_id = PeerId::from_public_key(&p2p_keypair.public());
        Multiaddr::from_str(&format!("/ip4/{}/tcp/{}/p2p/{peer_id}", ip_addr, port))
            .unwrap()
    };

    // given that we setup second producer
    let poa_secret = SecretKey::random(&mut rng);
    let mut config = config.clone();
    let p2p_keypair = convert_to_libp2p_keypair(&mut poa_secret.to_vec()).unwrap();
    config.consensus_key = Some(Secret::new(poa_secret.into()));
    config.block_production = Trigger::Interval {
        block_time: Duration::from_secs(2),
    };
    match config.p2p {
        Some(ref mut p2p) => {
            p2p.keypair = p2p_keypair.clone();
        }
        None => panic!("expected p2p config"),
    }

    // with configured `min_connected_reserved_peers` and `time_until_synced`
    config.min_connected_reserved_peers = 1;
    config.time_until_synced = Duration::from_secs(5);
    if let Some(p2p) = config.p2p.as_mut() {
        p2p.reserved_nodes = vec![first_producer_multi_addr.clone()];
    }

    let second_producer = FuelService::from_database(db.clone(), config.clone())
        .await
        .unwrap();

    let mut first_stream = first_producer.shared.block_importer.block_stream();
    let mut second_stream = second_producer.shared.block_importer.block_stream();

    loop {
        tokio::select! {
            Some(block_info) = first_stream.next() => {
                eprintln!("first producer produced a block {:?}", block_info);
            }
            Some(block_info) = second_stream.next() => {
                eprintln!("second producer got the block {:?}", block_info);
            }
        }
    }
}
