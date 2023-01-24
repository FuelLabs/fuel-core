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
    fuel_tx::{
        field::Inputs,
        TransactionBuilder,
        UtxoId,
    },
    fuel_vm::consts::REG_ONE,
};
use futures::StreamExt;
use rand::{
    rngs::StdRng,
    Rng,
    SeedableRng,
};
use std::sync::Arc;

#[tokio::test(flavor = "multi_thread")]
async fn test_nodes_syncing() {
    const N: usize = 10;
    let mut chain_config = ChainConfig::local_testnet();
    let mut rng = StdRng::seed_from_u64(11);
    let secret = fuel_core_types::fuel_crypto::SecretKey::random(&mut rng);
    let utxo_id: UtxoId = rng.gen();
    let initial_coin = ChainConfig::initial_coin(secret, 10000, Some(utxo_id));
    chain_config
        .initial_state
        .as_mut()
        .unwrap()
        .coins
        .as_mut()
        .unwrap()
        .push(initial_coin.clone());
    let mut nodes: Vec<_> = futures::stream::iter(0..N)
        .then(|i| {
            let chain_config = chain_config.clone();
            async move {
                let mut node_config = Config::local_node();
                node_config.chain_conf = chain_config.clone();
                node_config.utxo_validation = true;
                node_config.p2p.enable_mdns = true;
                if i == N - 1 {
                    node_config.block_production = Trigger::Instant;
                } else {
                    node_config.block_production = Trigger::Never;
                }
                let db = Database::in_memory();
                (
                    FuelService::from_database(db.clone(), node_config)
                        .await
                        .unwrap(),
                    db,
                )
            }
        })
        .take(N)
        .collect()
        .await;

    let producer = nodes.pop().unwrap();
    let node_subs: Vec<_> = nodes
        .iter()
        .map(|n| n.0.shared.block_importer.block_importer.subscribe())
        .collect();
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
    let tx_result = producer
        .0
        .shared
        .txpool
        .insert(vec![Arc::new(tx)])
        .pop()
        .unwrap()
        .unwrap();
    assert!(tx_result.removed.is_empty());

    for mut sub in node_subs {
        sub.recv().await.unwrap();
    }

    for (_n, db) in &mut nodes {
        let r = db.all_transactions(None, None).any(|tx| {
            tx.unwrap().as_script().map_or(false, |s| {
                s.inputs().iter().any(|i| *i.utxo_id().unwrap() == utxo_id)
            })
        });
        assert!(r);
    }
}
