use fuel_core::service::FuelService;
use fuel_core_client::client::{
    pagination::{
        PageDirection,
        PaginationRequest,
    },
    FuelClient,
};
use fuel_core_poa::Trigger;
use fuel_core_types::{
    fuel_asm::*,
    fuel_crypto::SecretKey,
    secrecy::Secret,
    signer::SignMode,
};
use rand::{
    rngs::StdRng,
    SeedableRng,
};
use test_helpers::{
    assemble_tx::AssembleAndRunTx,
    config_with_fee,
    default_signing_wallet,
};

#[tokio::test]
async fn poa_instant_trigger_is_produces_instantly() {
    let mut rng = StdRng::seed_from_u64(10);

    let mut config = config_with_fee();
    config.consensus_signer =
        SignMode::Key(Secret::new(SecretKey::random(&mut rng).into()));
    config.block_production = Trigger::Instant;

    let srv = FuelService::new_node(config).await.unwrap();

    let client = FuelClient::from(srv.bound_address);

    for i in 0..10usize {
        let script = vec![op::movi(0x10, i.try_into().unwrap())];
        client
            .run_script(script, vec![], default_signing_wallet())
            .await
            .unwrap();
        let count = client
            .blocks(PaginationRequest {
                cursor: None,
                results: 20,
                direction: PageDirection::Forward,
            })
            .await
            .expect("blocks request failed")
            .results
            .len();

        let block_number = i + 1;
        assert_eq!(count, block_number + 1 /* genesis block */);
    }
}
