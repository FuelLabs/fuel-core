use fuel_core::{
    chain_config::ChainConfig,
    database::Database,
    service::{
        Config,
        FuelService,
    },
};
use fuel_core_poa::Trigger;
use fuel_core_types::{
    fuel_asm::Opcode,
    fuel_crypto::SecretKey,
    fuel_tx::{
        Input,
        Transaction,
        TransactionBuilder,
        UtxoId,
    },
    fuel_types::Address,
    fuel_vm::consts::REG_ONE,
    secrecy::Secret,
    services::block_importer::ImportResult,
};
use futures::StreamExt;
use rand::{
    rngs::StdRng,
    Rng,
    SeedableRng,
};
use std::{
    collections::HashSet,
    sync::Arc,
};

#[derive(Clone)]
struct ProducerSetup {
    secret: SecretKey,
    num_test_txs: usize,
}

#[derive(Clone)]
struct ValidatorSetup {
    pub_key: Address,
}

struct Node {
    node: FuelService,
    db: Database,
    test_txs: Vec<Transaction>,
    block_subscription: tokio::sync::broadcast::Receiver<Arc<ImportResult>>,
}

struct Nodes {
    producers: Vec<Node>,
    validators: Vec<Node>,
}

async fn make_nodes(
    producers: impl IntoIterator<Item = Option<ProducerSetup>>,
    validators: impl IntoIterator<Item = Option<ValidatorSetup>>,
) -> Nodes {
    let producers: Vec<_> = producers.into_iter().collect();

    let mut rng = StdRng::seed_from_u64(11);

    let txs_coins: Vec<_> = producers
        .iter()
        .map(|p| {
            let num_test_txs = p.as_ref()?.num_test_txs;
            let all: Vec<_> = (0..num_test_txs)
                .map(|_| {
                    let secret = SecretKey::random(&mut rng);
                    let utxo_id: UtxoId = rng.gen();
                    let initial_coin =
                        ChainConfig::initial_coin(secret, 10000, Some(utxo_id));
                    let tx = TransactionBuilder::script(
                        vec![Opcode::RET(REG_ONE)].into_iter().collect(),
                        vec![],
                    )
                    .gas_limit(100000)
                    .add_unsigned_coin_input(
                        secret,
                        utxo_id,
                        initial_coin.amount,
                        initial_coin.asset_id,
                        Default::default(),
                        0,
                    )
                    .finalize_as_transaction();

                    (tx, initial_coin)
                })
                .collect();
            Some(all)
        })
        .collect();

    let mut producers_with_txs = Vec::with_capacity(producers.len());
    let mut chain_config = ChainConfig::local_testnet();

    for (all, producer) in txs_coins.into_iter().zip(producers.into_iter()) {
        match all {
            Some(all) => {
                let mut txs = Vec::with_capacity(all.len());
                for (tx, initial_coin) in all {
                    txs.push(tx);
                    chain_config
                        .initial_state
                        .as_mut()
                        .unwrap()
                        .coins
                        .as_mut()
                        .unwrap()
                        .push(initial_coin);
                }
                producers_with_txs.push(Some((producer.unwrap(), txs)));
            }
            None => {
                producers_with_txs.push(None);
            }
        }
    }

    let producers: Vec<_> = futures::stream::iter(producers_with_txs.into_iter())
        .then(|s| {
            let chain_config = chain_config.clone();
            async move {
                let mut node_config = Config::local_node();
                node_config.chain_conf = chain_config.clone();
                node_config.utxo_validation = true;
                node_config.p2p.enable_mdns = true;
                node_config.block_production = Trigger::Instant;
                let mut test_txs = Vec::with_capacity(0);
                if let Some((ProducerSetup { secret, .. }, txs)) = s {
                    match &mut node_config.chain_conf.consensus {
                        fuel_core::chain_config::ConsensusConfig::PoA { signing_key } => {
                            *signing_key = Input::owner(&secret.public_key());
                        }
                    }

                    node_config.consensus_key = Some(Secret::new(secret.into()));

                    test_txs = txs;
                }
                let db = Database::in_memory();
                let node = FuelService::from_database(db.clone(), node_config)
                    .await
                    .unwrap();

                let block_subscription =
                    node.shared.block_importer.block_importer.subscribe();
                Node {
                    node,
                    db,
                    test_txs,
                    block_subscription,
                }
            }
        })
        .collect()
        .await;

    let validators: Vec<_> = futures::stream::iter(validators)
        .then(|s| {
            let chain_config = chain_config.clone();
            async move {
                let mut node_config = Config::local_node();
                node_config.chain_conf = chain_config.clone();
                node_config.utxo_validation = true;
                node_config.p2p.enable_mdns = true;
                node_config.block_production = Trigger::Never;
                let db = Database::in_memory();

                if let Some(ValidatorSetup { pub_key }) = s {
                    match &mut node_config.chain_conf.consensus {
                        fuel_core::chain_config::ConsensusConfig::PoA { signing_key } => {
                            *signing_key = pub_key;
                        }
                    }
                }
                let node = FuelService::from_database(db.clone(), node_config)
                    .await
                    .unwrap();

                let block_subscription =
                    node.shared.block_importer.block_importer.subscribe();
                Node {
                    node,
                    db,
                    test_txs: vec![],
                    block_subscription,
                }
            }
        })
        .collect()
        .await;

    Nodes {
        producers,
        validators,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn test_nodes_syncing() {
    let mut rng = StdRng::seed_from_u64(12);

    let secret = SecretKey::random(&mut rng);
    let pub_key = Input::owner(&secret.public_key());
    let Nodes {
        producers,
        mut validators,
    } = make_nodes(
        [Some(ProducerSetup {
            secret,
            num_test_txs: 1,
        })],
        std::iter::repeat(Some(ValidatorSetup { pub_key })).take(10),
    )
    .await;

    let mut total_txs = 0;
    let mut expected = HashSet::new();
    for Node { node, test_txs, .. } in &producers {
        for tx in test_txs {
            let tx_result = node
                .shared
                .txpool
                .insert(vec![Arc::new(tx.clone())])
                .pop()
                .unwrap()
                .unwrap();

            total_txs += 1;
            expected.insert(Transaction::from(tx_result.inserted.as_ref()));

            assert!(tx_result.removed.is_empty());
        }
    }

    for (
        i,
        Node {
            db,
            block_subscription,
            ..
        },
    ) in validators.iter_mut().enumerate()
    {
        for _ in 0..total_txs {
            block_subscription.recv().await.unwrap();
        }
        let all_txs: Vec<_> = db
            .all_transactions(None, None)
            .map(Result::unwrap)
            .filter(|tx| tx.is_script())
            .collect();
        let count_missing: Vec<_> = db
            .all_transactions(None, None)
            .map(Result::unwrap)
            .filter(|tx| tx.is_script())
            .filter(|tx| expected.contains(tx))
            .collect();
        assert_eq!(
            count_missing.len(),
            0,
            "Node {} is Missing {:?}\nHAVE:\n{:?}\nNEED\n{:?}",
            i,
            count_missing,
            all_txs,
            expected,
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn test_multiple_same_key() {
    let mut rng = StdRng::seed_from_u64(12);

    let secret = SecretKey::random(&mut rng);
    let pub_key = Input::owner(&secret.public_key());
    let Nodes {
        producers,
        mut validators,
    } = make_nodes(
        std::iter::repeat(Some(ProducerSetup {
            secret,
            num_test_txs: 1,
        }))
        .take(3),
        std::iter::repeat(Some(ValidatorSetup { pub_key })).take(10),
    )
    .await;

    let mut total_txs = 0;
    let mut expected = HashSet::new();
    for Node { node, test_txs, .. } in &producers {
        for tx in test_txs {
            let tx_result = node
                .shared
                .txpool
                .insert(vec![Arc::new(tx.clone())])
                .pop()
                .unwrap()
                .unwrap();

            total_txs += 1;
            expected.insert(Transaction::from(tx_result.inserted.as_ref()));

            assert!(tx_result.removed.is_empty());
        }
    }

    for Node {
        db,
        block_subscription,
        ..
    } in &mut validators
    {
        for _ in 0..total_txs {
            block_subscription.recv().await.unwrap();
        }
        let count_missing: Vec<_> = db
            .all_transactions(None, None)
            .map(Result::unwrap)
            .filter(|tx| tx.is_script())
            .filter(|tx| expected.contains(tx))
            .collect();
        assert_eq!(count_missing.len(), 0, "Missing {:?}", count_missing);
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn test_multiple_producers() {
    let mut rng = StdRng::seed_from_u64(12);

    let secrets: Vec<_> = (0..3).map(|_| SecretKey::random(&mut rng)).collect();
    let pub_keys = secrets
        .clone()
        .into_iter()
        .map(|secret| Input::owner(&secret.public_key()));
    let Nodes {
        producers,
        mut validators,
    } = make_nodes(
        secrets.into_iter().map(|secret| {
            Some(ProducerSetup {
                secret,
                num_test_txs: 1,
            })
        }),
        pub_keys
            .into_iter()
            .flat_map(|pub_key| (0..10).map(move |_| Some(ValidatorSetup { pub_key }))),
    )
    .await;

    let mut groups = Vec::new();
    for Node { node, test_txs, .. } in &producers {
        let mut total_txs = 0;
        let mut expected = HashSet::new();
        for tx in test_txs {
            let tx_result = node
                .shared
                .txpool
                .insert(vec![Arc::new(tx.clone())])
                .pop()
                .unwrap()
                .unwrap();

            total_txs += 1;
            expected.insert(Transaction::from(tx_result.inserted.as_ref()));

            assert!(tx_result.removed.is_empty());
        }
        groups.push((total_txs, expected));
    }

    for (chunk, (total_txs, expected)) in validators.chunks_exact_mut(10).zip(groups) {
        for Node {
            db,
            block_subscription,
            ..
        } in chunk
        {
            for _ in 0..total_txs {
                block_subscription.recv().await.unwrap();
            }
            let count_missing: Vec<_> = db
                .all_transactions(None, None)
                .map(Result::unwrap)
                .filter(|tx| tx.is_script())
                .filter(|tx| expected.contains(tx))
                .collect();
            assert_eq!(count_missing.len(), 0, "Missing {:?}", count_missing);
        }
    }
}
