fn generate_transactions(
    nb_txs: u64,
    rng: &mut StdRng,
    secret_key: SecretKey,
) -> Vec<Transaction> {
    let mut transactions = Vec::with_capacity(nb_txs as usize);
    for _ in 0..nb_txs {
        let predicate = op::ret(RegId::ONE).to_bytes().to_vec();
        let owner = Input::predicate_owner(&predicate);
        let mut tx = TransactionBuilder::script(vec![], vec![])
            .script_gas_limit(10000)
            .add_unsigned_coin_input(
                secret_key,
                rng.gen(),
                1000,
                Default::default(),
                Default::default(),
            )
            .add_input(Input::coin_predicate(
                rng.gen(),
                owner,
                1000,
                Default::default(),
                Default::default(),
                Default::default(),
                predicate.clone(),
                vec![],
            ))
            .add_output(Output::coin(rng.gen(), 50, AssetId::default()))
            .add_output(Output::change(rng.gen(), 0, AssetId::default()))
            .finalize();
        tx.estimate_predicates(
            &checked_parameters(),
            MemoryInstance::new(),
            &EmptyStorage,
        )
        .expect("Predicate check failed");
        transactions.push(tx.into());
    }
    transactions
}

use fuel_core::{
    service::config::Trigger,
    upgradable_executor::native_executor::ports::TransactionExt,
};
use fuel_core_chain_config::CoinConfig;
use fuel_core_storage::transactional::AtomicView;
use fuel_core_types::{
    fuel_asm::{
        op,
        RegId,
    },
    fuel_crypto::*,
    fuel_tx::{
        input::coin::{
            CoinPredicate,
            CoinSigned,
        },
        AssetId,
        Finalizable,
        Input,
        Output,
        Transaction,
        TransactionBuilder,
    },
    fuel_vm::{
        checked_transaction::{
            CheckPredicateParams,
            EstimatePredicates,
        },
        interpreter::MemoryInstance,
        predicate::EmptyStorage,
    },
};
use rand::{
    rngs::StdRng,
    Rng,
    SeedableRng,
};
use test_helpers::builder::{
    local_chain_config,
    TestContext,
    TestSetupBuilder,
};
fn checked_parameters() -> CheckPredicateParams {
    local_chain_config().consensus_parameters.into()
}

fuel_core_trace::enable_tracing!();

fn main() {
    let n = std::env::var("BENCH_TXS_NUMBER")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap();

    #[cfg(feature = "parallel-executor")]
    let number_of_cores = std::env::var("FUEL_BENCH_CORES")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap();

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let _drop = rt.enter();

    let mut rng = rand::rngs::StdRng::seed_from_u64(2322u64);

    let start_transaction_generation = std::time::Instant::now();
    let secret_key = SecretKey::random(&mut rng);
    let transactions = generate_transactions(n, &mut rng, secret_key);
    println!(
        "Generated {} transactions in {:?} ms.",
        n,
        start_transaction_generation.elapsed().as_millis()
    );

    let mut test_builder = TestSetupBuilder::new(2322);
    // setup genesis block with coins that transactions can spend
    // We don't use the function to not have to convert Script to transactions
    test_builder.initial_coins.extend(
        transactions
            .iter()
            .flat_map(|t| t.inputs().unwrap())
            .filter_map(|input| {
                if let Input::CoinSigned(CoinSigned {
                    amount,
                    owner,
                    asset_id,
                    utxo_id,
                    tx_pointer,
                    ..
                })
                | Input::CoinPredicate(CoinPredicate {
                    amount,
                    owner,
                    asset_id,
                    utxo_id,
                    tx_pointer,
                    ..
                }) = input
                {
                    Some(CoinConfig {
                        tx_id: *utxo_id.tx_id(),
                        output_index: utxo_id.output_index(),
                        tx_pointer_block_height: tx_pointer.block_height(),
                        tx_pointer_tx_idx: tx_pointer.tx_index(),
                        owner: *owner,
                        amount: *amount,
                        asset_id: *asset_id,
                    })
                } else {
                    None
                }
            }),
    );
    // Save coins to a file
    let value = bincode::serialize(&test_builder.initial_coins).unwrap();
    std::fs::write("coins.bin", value).unwrap();

    // disable automated block production
    test_builder.trigger = Trigger::Never;
    test_builder.utxo_validation = true;
    test_builder.gas_limit = Some(10_000_000_000);
    test_builder.block_size_limit = Some(1_000_000_000_000);
    test_builder.max_txs = transactions.len();
    #[cfg(feature = "parallel-executor")]
    {
        test_builder.number_threads_pool_verif = number_of_cores;
        test_builder.executor_number_of_cores = number_of_cores;
    }

    // spin up node
    let _ = rt.block_on(async move {
        // start the producer node
        let TestContext { srv, client, .. } = test_builder.finalize().await;

        // insert all transactions
        let mut subscriber = srv
            .shared
            .txpool_shared_state
            .new_tx_notification_subscribe();
        let mut nb_left = n;
        let start_insertion = std::time::Instant::now();
        srv.shared
            .txpool_shared_state
            .try_insert(transactions)
            .unwrap();
        while nb_left > 0 {
            let _ = subscriber.recv().await.unwrap();
            nb_left -= 1;
        }
        println!(
            "Inserted {} transactions in {:?} ms.",
            n,
            start_insertion.elapsed().as_millis()
        );
        client.produce_blocks(1, None).await.unwrap();
        let block = srv
            .shared
            .database
            .on_chain()
            .latest_view()
            .unwrap()
            .get_sealed_block_by_height(&1.into())
            .unwrap()
            .unwrap();
        let value = bincode::serialize(&block).unwrap();
        // Write it to a file
        std::fs::write("block.bin", value).unwrap();
    });
}
