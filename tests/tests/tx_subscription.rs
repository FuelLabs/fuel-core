#![allow(clippy::arithmetic_side_effects)]
use fuel_core::{
    chain_config::TESTNET_WALLET_SECRETS,
    p2p_test_helpers::{
        BootstrapSetup,
        CustomizeConfig,
        Nodes,
        ProducerSetup,
        ValidatorSetup,
        make_nodes,
    },
};
use fuel_core_client::client::{
    FuelClient,
    types::TransactionStatus,
};
use fuel_core_types::{
    fuel_tx::*,
    fuel_vm::*,
};
use rand::{
    SeedableRng,
    rngs::StdRng,
};
use std::str::FromStr;
use test_helpers::assemble_tx::{
    AssembleAndRunTx,
    SigningAccount,
};

#[rstest::rstest]
#[tokio::test(flavor = "multi_thread")]
async fn test_tx_gossip_subscribe_to_transactions(
    #[values(true, false)] subscribe_to_transactions: bool,
) {
    let mut rng = StdRng::seed_from_u64(1234);

    let producer_secret = SecretKey::random(&mut rng);
    let producer_pub_key = Input::owner(&producer_secret.public_key());
    let validator_secret = SecretKey::random(&mut rng);
    let validator_pub_key = Input::owner(&validator_secret.public_key());

    // Given
    let overrides = CustomizeConfig::no_overrides()
        .subscribe_to_transactions(subscribe_to_transactions);
    let Nodes {
        producers,
        validators,
        bootstrap_nodes: _dont_drop,
    } = make_nodes(
        vec![
            Some(BootstrapSetup::new(producer_pub_key)),
            Some(BootstrapSetup::new(validator_pub_key)),
        ],
        vec![Some(
            ProducerSetup::new_with_overrides(producer_secret, overrides.clone())
                .with_name(format!("{}:producer", producer_pub_key)),
        )],
        vec![Some(
            ValidatorSetup::new_with_overrides(producer_pub_key, overrides.clone())
                .with_name(format!("{}:validator", validator_pub_key)),
        )],
        None,
    )
    .await;

    let producer = FuelClient::from(producers[0].node.bound_address);
    let validator = FuelClient::from(validators[0].node.bound_address);

    let consensus_parameters = producer
        .chain_info()
        .await
        .expect("Could not get chain info")
        .consensus_parameters;
    let chain_id = consensus_parameters.chain_id();
    let base_asset_id = *consensus_parameters.base_asset_id();

    let wallet =
        SigningAccount::Wallet(SecretKey::from_str(TESTNET_WALLET_SECRETS[0]).unwrap());

    let tx = producer
        .assemble_transfer(wallet, vec![(Address::new([1u8; 32]), base_asset_id, 1234)])
        .await
        .unwrap();
    let tx_id = tx.id(&chain_id);

    // When
    validator
        .submit(&tx)
        .await
        .expect("Transaction submission to validator failed");
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    let response = producer.transaction_status(&tx_id).await;

    // Then
    if subscribe_to_transactions {
        // Gossip should have propagated the transaction to the producer
        assert!(matches!(response, Ok(TransactionStatus::Success { .. })));
    } else {
        // No gossip, so tx should not be found on the producer
        let err = response.expect_err("Expected error when fetching transaction status");
        assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
    }
}
