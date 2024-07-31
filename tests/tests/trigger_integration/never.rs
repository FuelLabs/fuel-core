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
    fuel_asm::op,
    fuel_crypto::SecretKey,
    fuel_tx::TransactionBuilder,
    secrecy::Secret,
};
use rand::{
    rngs::StdRng,
    SeedableRng,
};

#[tokio::test(start_paused = true)]
async fn poa_never_trigger_doesnt_produce_blocks() {
    let mut rng = StdRng::seed_from_u64(10);
    let db = Database::default();
    let mut config = Config::local_node();
    config.block_production = Trigger::Never;
    config.consensus_signer =
        SignMode::Key(Secret::new(SecretKey::random(&mut rng).into()));
    let srv = FuelService::from_database(db.clone(), config)
        .await
        .unwrap();
    let client = FuelClient::from(srv.bound_address);

    for i in 0..10 {
        let tx =
            TransactionBuilder::script([op::movi(0x10, i)].into_iter().collect(), vec![])
                .add_random_fee_input()
                .finalize_as_transaction();
        let _tx_id = client.submit(&tx).await.unwrap();
        tokio::time::advance(tokio::time::Duration::new(10, 0)).await;
        let resp = client
            .blocks(PaginationRequest {
                cursor: None,
                results: 20,
                direction: PageDirection::Forward,
            })
            .await
            .expect("blocks request failed");
        assert_eq!(resp.results.len(), 1 /* only genesis block */);
    }
}
