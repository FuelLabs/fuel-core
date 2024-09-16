use fuel_core::{
    combined_database::CombinedDatabase,
    service::{
        adapters::consensus_module::poa::block_path,
        Config,
        FuelService,
    },
};
use fuel_core_client::client::{
    types::TransactionStatus,
    FuelClient,
};
use fuel_core_poa::signer::SignMode;
use fuel_core_storage::transactional::AtomicView;
use fuel_core_types::{
    blockchain::consensus::Consensus,
    fuel_crypto::SecretKey,
    fuel_tx::Transaction,
    secrecy::Secret,
};
use rand::{
    rngs::StdRng,
    SeedableRng,
};
use tempfile::tempdir;
use test_helpers::{
    fuel_core_driver::FuelCoreDriver,
    produce_block_with_tx,
};

#[tokio::test]
async fn can_get_da_compressed_blocks() {
    let mut rng = StdRng::seed_from_u64(10);
    let poa_secret = SecretKey::random(&mut rng);
    let poa_public = poa_secret.public_key();

    let db = CombinedDatabase::default();
    let mut config = Config::local_node();
    config.consensus_signer = SignMode::Key(Secret::new(poa_secret.into()));
    let srv = FuelService::from_combined_database(db.clone(), config)
        .await
        .unwrap();
    let client = FuelClient::from(srv.bound_address);

    let status = client
        .submit_and_await_commit(&Transaction::default_test_tx())
        .await
        .unwrap();

    let block_height = match status {
        TransactionStatus::Success { block_height, .. } => block_height,
        _ => {
            panic!("unexpected result")
        }
    };

    let block = client.get_da_compressed_block(block_height).await.unwrap();
}
