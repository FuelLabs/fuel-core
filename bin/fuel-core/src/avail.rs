#[cfg(not(any(feature = "p2p", feature = "relayer")))]
#[cfg(feature = "avail")]
#[tokio::main(flavor = "multi_thread")]
async fn main() -> anyhow::Result<()> {
    // Use Jemalloc for main binary
    #[global_allocator]
    static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

    use std::{
        env::current_dir,
        str::FromStr,
    };

    use fuel_core::{
        fuel_core_graphql_api::DEFAULT_QUERY_COSTS,
        service::DbType,
    };
    use fuel_core_bin::cli::run::{
        self,
        consensus::{
            Boolean,
            Instant,
            Interval,
            Open,
        },
    };

    let ip = "127.0.0.1";
    let port = 4000;

    // fuel_core_bin::cli::init_logging();

    #[allow(deprecated)]
    let cmd = run::Command {
        service_name: "avail-fuel-core".to_string(),
        max_database_cache_size: Some(100_000_000),
        database_path: current_dir()?.join(".avail-db"),
        database_type: DbType::RocksDb,
        rocksdb_max_fds: -1,
        state_rewind_duration: humantime::Duration::from_str("0s")?,
        snapshot: None, // probably need custom snapshot
        db_prune: true,
        continue_on_error: true,
        debug: false,
        historical_execution: false,
        vm_backtrace: false,
        utxo_validation: true,
        native_executor_version: None,
        gas_price: run::gas_price::GasPriceArgs {
            starting_gas_price: 0,
            gas_price_change_percent: 0,
            min_gas_price: 0,
            gas_price_threshold_percent: 0,
            min_da_gas_price: 0,
            max_da_gas_price: 0,
            da_gas_price_p_component: 0,
            da_gas_price_d_component: 0,
            da_committer_url: None,
            da_poll_interval: None,
            da_starting_recorded_height: None,
        },
        consensus_key: Some(
            "c6ddac5bce7b205c88472070cda497a3f88c9807e3c4e0b31f8e49768d988129"
                .to_string(),
        ),
        da_compression: Some(humantime::Duration::from_str("14d")?), /* necessary for avail */
        poa_trigger: run::consensus::PoATriggerArgs {
            instant: Instant {
                instant: Boolean::False,
            },
            interval: Interval {
                block_time: Some(humantime::Duration::from_str("1s")?), /* necessary for avail */
            },
            open: Open { period: None },
        },
        predefined_blocks_path: None,
        coinbase_recipient: None,
        tx_pool: run::tx_pool::TxPoolArgs {
            tx_pool_ttl: humantime::Duration::from_str("30s")?, // could be tuned
            tx_ttl_check_interval: humantime::Duration::from_str("10s")?, /* could be tuned */
            tx_max_number: 200_000, // could be tuned
            tx_max_total_gas: 1_000_000_000_000_000,
            tx_max_total_bytes: 1_000_000_000_000_000,
            tx_max_chain_count: 32,
            tx_blacklist_addresses: vec![],
            tx_blacklist_coins: vec![],
            tx_blacklist_messages: vec![],
            tx_blacklist_contracts: vec![],
            tx_number_threads_to_verify_transactions: 8, // could be tuned
            tx_size_of_verification_queue: 200_000,      // could be tuned
            tx_number_threads_p2p_sync: 0,
            tx_size_of_p2p_sync_queue: 0,
            tx_max_pending_write_requests: 2_000_000, // could be tuned
            tx_max_pending_read_requests: 2_000_000,  // could be tuned
            tx_pending_pool_ttl: humantime::Duration::from_str("30s")?, // could be tuned
            tx_pending_pool_size_percentage: 50,      // could be tuned
        },
        tx_status_manager: run::tx_status_manager::TxStatusManagerArgs {
            tx_number_active_subscriptions: 1_000_000, // could be tuned
            status_cache_ttl: humantime::Duration::from_str("30s")?, // could be tuned
        },
        graphql: run::graphql::GraphQLArgs {
            ip: ip.parse()?,
            port,
            graphql_number_of_threads: 10, // could be tuned
            database_batch_size: 2_000,
            graphql_max_depth: 16,
            graphql_max_complexity: 800_000,
            graphql_max_recursive_depth: 24,
            max_queries_resolver_recursive_depth: 1,
            max_queries_directives: 10,
            graphql_max_concurrent_queries: 200_000_000, // could be tuned
            graphql_request_body_bytes_limit: 1048576,
            query_log_threshold_time: humantime::Duration::from_str("2s")?,
            api_request_timeout: humantime::Duration::from_str("100s")?,
            assemble_tx_dry_run_limit: 3,
            assemble_tx_estimate_predicates_limit: 10,
            required_fuel_block_height_tolerance: 10,
            required_fuel_block_height_timeout: humantime::Duration::from_str("30s")?,
            costs: run::graphql::QueryCosts {
                balance_query: DEFAULT_QUERY_COSTS.balance_query,
                coins_to_spend: DEFAULT_QUERY_COSTS.coins_to_spend,
                get_peers: DEFAULT_QUERY_COSTS.get_peers,
                estimate_predicates: DEFAULT_QUERY_COSTS.estimate_predicates,
                dry_run: DEFAULT_QUERY_COSTS.dry_run,
                assemble_tx: DEFAULT_QUERY_COSTS.assemble_tx,
                storage_read_replay: DEFAULT_QUERY_COSTS.storage_read_replay,
                submit: DEFAULT_QUERY_COSTS.submit,
                submit_and_await: DEFAULT_QUERY_COSTS.submit_and_await,
                status_change: DEFAULT_QUERY_COSTS.status_change,
                storage_read: DEFAULT_QUERY_COSTS.storage_read,
                tx_get: DEFAULT_QUERY_COSTS.tx_get,
                tx_status_read: DEFAULT_QUERY_COSTS.tx_status_read,
                tx_raw_payload: DEFAULT_QUERY_COSTS.tx_raw_payload,
                block_header: DEFAULT_QUERY_COSTS.block_header,
                block_transactions: DEFAULT_QUERY_COSTS.block_transactions,
                block_transactions_ids: DEFAULT_QUERY_COSTS.block_transactions_ids,
                storage_iterator: DEFAULT_QUERY_COSTS.storage_iterator,
                bytecode_read: DEFAULT_QUERY_COSTS.bytecode_read,
                state_transition_bytecode_read: DEFAULT_QUERY_COSTS
                    .state_transition_bytecode_read,
                da_compressed_block_read: DEFAULT_QUERY_COSTS.da_compressed_block_read,
            },
        },
        metrics: vec![fuel_core_metrics::config::Module::All], // not necessary for avail
        max_da_lag: 10,
        max_wait_time: humantime::Duration::from_str("30s")?,
        min_connected_reserved_peers: 0,
        time_until_synced: humantime::Duration::from_str("0s")?,
        production_timeout: humantime::Duration::from_str("20s")?,
        memory_pool_size: 32, // could be tuned
        profiling: run::profiling::ProfilingArgs {
            pyroscope_url: None,
            pprof_sample_rate: 0,
        },
    };

    let fuel_core_task = tokio::spawn(async move {
        let _ = run::exec(cmd).await;
    });
    let observer_task = tokio::spawn(async move {
        let client = fuel_core_client::client::FuelClient::new(format!(
            "http://{}:{}/v1/graphql",
            ip, port
        ))
        .unwrap(); // qed
        let mut last_block = 0;
        let mut peak_tps = 0;

        let multi = indicatif::MultiProgress::new();
        let block_bar = multi.add(indicatif::ProgressBar::new(1));
        block_bar.set_style(
            indicatif::ProgressStyle::default_bar()
                .template("Block #{msg} | {prefix} peak tps")
                .unwrap(),
        );

        let progress_units = 100_000;
        let tx_bar = multi.add(indicatif::ProgressBar::new(progress_units));
        tx_bar.set_style(
            indicatif::ProgressStyle::default_bar()
                .template("[{bar:100}] {pos} tx(s)")
                .unwrap()
                .progress_chars("█▉▊▋▌▍▎▏ "),
        );

        loop {
            tokio::select! {
                biased;

                _ = tokio::signal::ctrl_c() => {
                    println!("\nStopping observer");
                    break
                }

                _ = tokio::time::sleep(std::time::Duration::from_secs(5)) => {
                    let block_res = client.blocks(fuel_core_client::client::pagination::PaginationRequest {
                        cursor: None,
                        results: 10,
                        direction: fuel_core_client::client::pagination::PageDirection::Backward,
                    }).await;

                    match block_res {
                        Ok(block) => {
                            for block in block.results.iter().rev() {
                                if block.header.height > last_block {
                                    last_block = block.header.height;
                                    let tx_count = block.transactions.len() as u64;
                                    if tx_count > peak_tps {
                                        peak_tps = tx_count;
                                    }
                                    block_bar.set_message(format!("{}", last_block));
                                    block_bar.set_prefix(format!("{}", peak_tps));
                                    tx_bar.set_position(tx_count);
                                }
                            }
                        }
                        Err(_) => {
                            continue;
                        }
                    }
                }
            }
        }
    });

    tokio::try_join!(fuel_core_task, observer_task)?;

    Ok(())
}

#[cfg(any(feature = "p2p", feature = "relayer"))]
#[cfg(not(feature = "avail"))]
fn main() {
    panic!("The feature `p2p`, `relayer` & `rocksdb` are not supported in this binary. Only feature `avail` is supported. Run with `cargo run --bin fuel-core-avail -p fuel-core-bin --no-default-features --features avail`");
}
