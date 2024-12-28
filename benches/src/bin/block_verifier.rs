use fuel_core::{
    service::config::Trigger,
    state::historical_rocksdb::StateRewindPolicy,
};
use fuel_core_chain_config::CoinConfig;
use fuel_core_storage::transactional::AtomicView;
use fuel_core_types::{
    blockchain::{
        block::Block,
        consensus::Sealed,
    },
    fuel_asm::{
        op,
        RegId,
    },
    fuel_tx::{
        Finalizable,
        Input,
        Output,
        TransactionBuilder,
    },
    fuel_types::AssetId,
    fuel_vm::{
        checked_transaction::{
            CheckPredicateParams,
            EstimatePredicates,
        },
        interpreter::MemoryInstance,
        predicate::EmptyStorage,
        SecretKey,
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

fn read_block() -> Sealed<Block> {
    // Get it from the file block.bin (bincode encoded)
    let file = std::fs::File::open("block.bin").unwrap();
    let block: Sealed<Block> = bincode::deserialize_from(file).unwrap();
    block
}

fn read_coins() -> Vec<CoinConfig> {
    // Get it from the file coins.bin (bincode encoded)
    let file = std::fs::File::open("coins.bin").unwrap();
    let coins: Vec<CoinConfig> = bincode::deserialize_from(file).unwrap();
    coins
}

fn checked_parameters() -> CheckPredicateParams {
    local_chain_config().consensus_parameters.into()
}

fn main() {
    #[cfg(feature = "parallel-executor")]
    let number_of_cores = std::env::var("FUEL_BENCH_CORES")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap();
    let n = std::env::var("BENCH_TXS_NUMBER")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let _drop = rt.enter();

    let mut rng = rand::rngs::StdRng::seed_from_u64(2322u64);

    let generator = |rng: &mut StdRng| {
        let predicate = op::ret(RegId::ONE).to_bytes().to_vec();
        let owner = Input::predicate_owner(&predicate);
        let mut tx = TransactionBuilder::script(vec![], vec![])
            .script_gas_limit(10000)
            .add_unsigned_coin_input(
                SecretKey::random(rng),
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
        tx
    };

    let mut transactions = vec![];
    for _ in 0..n {
        transactions.push(generator(&mut rng));
    }

    let mut test_builder = TestSetupBuilder::new(2322);
    test_builder.config_coin_inputs_from_transactions(&transactions);
    test_builder.trigger = Trigger::Never;
    test_builder.utxo_validation = true;
    test_builder.gas_limit = Some(10_000_000_000);
    test_builder.block_size_limit = Some(1_000_000_000_000);
    test_builder.max_txs = transactions.len();
    test_builder.state_rewind_policy = Some(StateRewindPolicy::NoRewind);
    #[cfg(feature = "parallel-executor")]
    {
        test_builder.number_threads_pool_verif = number_of_cores;
        test_builder.executor_number_of_cores = number_of_cores;
    }

    let sealed_block = rt.block_on({
        let mut test_builder = test_builder.clone();
        async move {
            let sealed_block = {
                // start the producer node
                let TestContext { srv, client, .. } = test_builder.finalize().await;

                // insert all transactions
                for tx in transactions {
                    srv.shared
                        .txpool_shared_state
                        .insert(tx.into())
                        .await
                        .unwrap();
                }
                let _ = client.produce_blocks(1, None).await;

                // sanity check block to ensure the transactions were actually processed
                let block = srv
                    .shared
                    .database
                    .on_chain()
                    .latest_view()
                    .unwrap()
                    .get_sealed_block_by_height(&1.into())
                    .unwrap()
                    .unwrap();
                assert_eq!(block.entity.transactions().len(), (n + 1) as usize);
                block
            };
            sealed_block
        }
    });

    rt.block_on(async move {
        let TestContext { srv, .. } = test_builder.finalize().await;

        let start = std::time::Instant::now();

        srv.shared
            .block_importer
            .execute_and_commit(sealed_block)
            .await
            .expect("Should validate the block");
        println!("Block imported in {:?}", start.elapsed().as_secs());
    });
}

fuel_core_trace::enable_tracing!();
