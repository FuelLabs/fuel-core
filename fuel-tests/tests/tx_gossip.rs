#[cfg(feature = "p2p")]
mod gossip_tests {
    use fuel_core::{
        chain_config::{
            CoinConfig,
            StateConfig,
        },
        fuel_p2p::config::convert_to_libp2p_keypair,
        service::{
            Config,
            FuelService,
        },
    };
    use fuel_core_interfaces::common::{
        fuel_tx::TransactionBuilder,
        fuel_vm::{
            consts::*,
            prelude::*,
        },
    };
    use fuel_gql_client::client::FuelClient;
    use rand::{
        rngs::StdRng,
        Rng,
        SeedableRng,
    };
    use std::time::Duration;

    #[tokio::test]
    async fn test_tx_gossiping() {
        let mut rng = StdRng::seed_from_u64(2322);

        let mut node_config = Config::local_node();
        node_config.utxo_validation = true;

        let tx = TransactionBuilder::script(
            Opcode::RET(REG_ONE).to_bytes().into_iter().collect(),
            vec![],
        )
        .gas_limit(100)
        .gas_price(1)
        .add_unsigned_coin_input(
            SecretKey::random(&mut rng),
            rng.gen(),
            1000,
            Default::default(),
            Default::default(),
            0,
        )
        .add_output(Output::Change {
            amount: 0,
            asset_id: Default::default(),
            to: rng.gen(),
        })
        .finalize();

        if let Input::CoinSigned {
            amount,
            owner,
            asset_id,
            utxo_id,
            ..
        }
        | Input::CoinPredicate {
            amount,
            owner,
            asset_id,
            utxo_id,
            ..
        } = tx.inputs()[0]
        {
            let mut initial_state = StateConfig::default();
            let coin_config = vec![CoinConfig {
                tx_id: Some(*utxo_id.tx_id()),
                output_index: Some(utxo_id.output_index() as u64),
                block_created: None,
                maturity: None,
                owner,
                amount,
                asset_id,
            }];
            initial_state.coins = Some(coin_config);
            node_config.chain_conf.initial_state = Some(initial_state);
        };

        node_config.p2p.enable_mdns = true;

        let node_one = FuelService::new_node(node_config.clone()).await.unwrap();
        let client_one = FuelClient::from(node_one.bound_address);

        let secret_key = rng.gen::<Bytes32>();
        node_config.p2p.local_keypair = convert_to_libp2p_keypair(secret_key).unwrap();
        let node_two = FuelService::new_node(node_config.clone()).await.unwrap();
        let client_two = FuelClient::from(node_two.bound_address);

        tokio::time::sleep(Duration::new(3, 0)).await;

        let result = client_one.submit(&tx).await.unwrap();

        let tx = client_one.transaction(&result.0.to_string()).await.unwrap();
        assert!(tx.is_some());

        // Perhaps some delay is needed before this query?

        tokio::time::sleep(Duration::new(3, 0)).await;

        let tx = client_two.transaction(&result.0.to_string()).await.unwrap();
        assert!(tx.is_some());
    }
}
