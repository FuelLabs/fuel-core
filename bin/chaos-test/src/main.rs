mod cli;
mod cluster;
mod fault;
mod invariants;
mod proxy;
mod redis_server;
mod timeline;

use std::{sync::Arc, time::Duration};

use clap::Parser;
use tokio::sync::Mutex;
use tracing::info;

use cli::Cli;
use cluster::Cluster;
use fault::FaultScheduler;
use timeline::{Timeline, TimelineEventKind};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Generate or use provided seed
    let seed = cli.seed.unwrap_or_else(|| {
        let s = rand::random::<u64>();
        println!("Generated random seed: {s}");
        s
    });

    // Init tracing
    let filter = tracing_subscriber::EnvFilter::try_new(&cli.log_level)
        .unwrap_or_else(|_| {
            tracing_subscriber::EnvFilter::new("info")
        });
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();

    info!("Chaos test starting");
    info!("  Seed: {seed}");
    info!("  Duration: {:?}", *cli.duration);
    info!("  Nodes: {}", cli.nodes);
    info!("  Redis nodes: {}", cli.redis_nodes);
    info!("  Block time: {:?}", *cli.block_time);
    info!("  Fault interval: {:?}", *cli.fault_interval);

    let timeline = Timeline::new(seed);
    timeline.record(TimelineEventKind::Info(format!(
        "Test started with seed={seed}, nodes={}, redis={}, block_time={:?}, fault_interval={:?}",
        cli.nodes, cli.redis_nodes, *cli.block_time, *cli.fault_interval
    )));

    // Create cluster
    let cluster = Cluster::new(
        cli.nodes,
        cli.redis_nodes,
        *cli.block_time,
        seed,
    )
    .await;
    let cluster = Arc::new(Mutex::new(cluster));

    // Wait for initial leader election
    info!("Waiting for initial leader election (up to 30s)...");
    let election_deadline =
        tokio::time::Instant::now() + Duration::from_secs(30);
    let mut leader_found = false;

    {
        use fuel_core_poa::ports::BlockImporter;
        use futures::StreamExt;

        let cluster_guard = cluster.lock().await;
        let live = cluster_guard.live_nodes();
        if let Some((_, node)) = live.first() {
            let mut stream =
                node.service.shared.block_importer.block_stream();
            loop {
                let remaining = election_deadline
                    .saturating_duration_since(tokio::time::Instant::now());
                if remaining.is_zero() {
                    break;
                }
                match tokio::time::timeout(remaining, stream.next()).await
                {
                    Ok(Some(block)) => {
                        if block.is_locally_produced() {
                            info!(
                                "Leader elected! First local block at height {}",
                                block.block_header.height()
                            );
                            leader_found = true;
                            break;
                        }
                    }
                    Ok(None) => break,
                    Err(_) => break,
                }
            }
        }
    }

    if !leader_found {
        // Try any node that has produced a block
        info!("No local block seen yet, checking if any blocks are being produced...");
        let cluster_guard = cluster.lock().await;
        let has_nodes = !cluster_guard.live_nodes().is_empty();
        drop(cluster_guard);
        if has_nodes {
            info!("Nodes are alive, proceeding with chaos test...");
        } else {
            anyhow::bail!("No nodes alive after startup");
        }
    }

    timeline.record(TimelineEventKind::Info(
        "Initial leader election complete, starting chaos".to_string(),
    ));

    // Create stop channels
    let (fault_stop_tx, fault_stop_rx) = tokio::sync::watch::channel(false);
    let (invariant_stop_tx, invariant_stop_rx) =
        tokio::sync::watch::channel(false);

    // Spawn invariant checker
    let invariant_cluster = cluster.clone();
    let invariant_timeline = timeline.clone();
    let lease_ttl = Duration::from_secs(2);
    let gap_tolerance = Duration::from_secs(60);
    let stall_threshold = *cli.stall_threshold;
    let invariant_handle = tokio::spawn(async move {
        invariants::run_invariant_checker(
            invariant_cluster,
            invariant_timeline,
            invariant_stop_rx,
            lease_ttl,
            gap_tolerance,
            stall_threshold,
        )
        .await;
    });

    // Spawn fault scheduler
    let fault_cluster = cluster.clone();
    let fault_timeline = timeline.clone();
    let node_count = cli.nodes;
    let redis_count = cli.redis_nodes;
    let fault_interval = *cli.fault_interval;
    let fault_handle = tokio::spawn(async move {
        let scheduler = FaultScheduler::new(
            seed,
            fault_interval,
            node_count,
            redis_count,
        );
        scheduler
            .run(fault_cluster, fault_timeline, fault_stop_rx)
            .await;
    });

    // Run for the configured duration or until Ctrl+C
    let test_duration = *cli.duration;
    tokio::select! {
        _ = tokio::time::sleep(test_duration) => {
            info!("Test duration elapsed");
        }
        _ = tokio::signal::ctrl_c() => {
            info!("Ctrl+C received, stopping...");
        }
    }

    // Stop fault scheduler
    info!("Stopping fault scheduler...");
    fault_stop_tx.send(true)?;
    fault_handle.await?;

    // Settling period: restore all proxies and wait
    info!("Settling period: restoring all proxies and waiting 10s...");
    {
        let cluster_guard = cluster.lock().await;
        cluster_guard.restore_all_proxies();
    }
    timeline.record(TimelineEventKind::Info(
        "Settling period: all proxies restored".to_string(),
    ));
    tokio::time::sleep(Duration::from_secs(10)).await;

    // Stop invariant checker
    info!("Stopping invariant checker...");
    invariant_stop_tx.send(true)?;
    invariant_handle.await?;

    // Print report
    timeline.print_report();

    // Determine exit code
    let violations = timeline.violations();
    if violations.is_empty() {
        info!("Chaos test PASSED");
        Ok(())
    } else {
        eprintln!(
            "\nChaos test FAILED with {} violation(s). Seed: {seed}",
            violations.len()
        );
        std::process::exit(1);
    }
}
