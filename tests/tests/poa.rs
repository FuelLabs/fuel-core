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
use fuel_core_types::{
    blockchain::consensus::Consensus,
    fuel_crypto::SecretKey,
    fuel_tx::Transaction,
    fuel_types::Bytes32,
    secrecy::Secret,
};
use rand::{
    rngs::StdRng,
    SeedableRng,
};
use std::str::FromStr;

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
        .submit_and_await_commit(&Transaction::default())
        .await
        .unwrap();
    let block_id = match status {
        TransactionStatus::Success { block_id, .. } => block_id,
        _ => {
            panic!("unexpected result")
        }
    };

    let block_id = Bytes32::from_str(&block_id).unwrap();

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
        .get_sealed_block(&block_id.into())
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
