use fuel_core::{
    service::config::Trigger,
    state::historical_rocksdb::StateRewindPolicy,
    upgradable_executor::native_executor::ports::TransactionExt,
};
use fuel_core_chain_config::CoinConfig;
use fuel_core_types::blockchain::{
    block::Block,
    consensus::Sealed,
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
    let block = read_block();
    println!("Block read");
    let coins = read_coins();
    println!("Coins read");
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let _drop = rt.enter();
    let mut test_builder = TestSetupBuilder::new(2322);
    let chain_conf = local_chain_config();
    test_builder.trigger = Trigger::Never;
    test_builder.utxo_validation = true;
    test_builder.gas_limit = Some(
        block
            .entity
            .transactions()
            .iter()
            .filter_map(|tx| {
                if tx.is_mint() {
                    return None;
                }
                Some(tx.max_gas(&chain_conf.consensus_parameters).unwrap())
            })
            .sum(),
    );
    test_builder.block_size_limit = Some(1_000_000_000_000_000);
    test_builder.max_txs = n as usize;
    test_builder.state_rewind_policy = Some(StateRewindPolicy::NoRewind);
    #[cfg(feature = "parallel-executor")]
    {
        test_builder.number_threads_pool_verif = number_of_cores;
        test_builder.executor_number_of_cores = number_of_cores;
    }
    test_builder.initial_coins.extend(coins);
    rt.block_on(async move {
        let TestContext { srv, .. } = test_builder.finalize().await;

        let start = std::time::Instant::now();

        srv.shared
            .block_importer
            .execute_and_commit(block)
            .await
            .expect("Should validate the block");
        println!("Block imported in {:?}", start.elapsed().as_secs());
    });
}

fuel_core_trace::enable_tracing!();
